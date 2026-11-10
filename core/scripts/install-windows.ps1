param(
  [string]$InstallDir = "$env:LOCALAPPDATA\memnest",
  [string]$DataDir = "$env:USERPROFILE\.memnest",
  [string]$TaskName = "Memnest",
  [int]$Port = 3111,
  [string]$BinPath = "",
  [string[]]$WatchPath = @(),
  [switch]$Autocontext,
  [switch]$NoClients,
  [switch]$NoVerify,
  [switch]$NoPath
)

# Per-user install: no elevation, no Windows service. Two scheduled tasks start
# the server and the conversation watcher at logon, as the current user, so the
# watcher can read that user's transcripts. Keep this file ASCII: Windows
# PowerShell 5.1 reads a script without a BOM in the system code page.
$ErrorActionPreference = "Stop"
# A trailing backslash would escape the closing quote of a quoted argument.
$InstallDir = $InstallDir.TrimEnd("\")
$DataDir = $DataDir.TrimEnd("\")
$WatchPath = @($WatchPath | ForEach-Object { $_.TrimEnd("\") })
# install.ps1 runs this file as a script block from the unpacked archive root,
# where there is no script path to read.
$ScriptDir = $PSScriptRoot
if (-not $ScriptDir) { $ScriptDir = Join-Path (Get-Location).Path "scripts" }
$Root = Split-Path -Parent $ScriptDir
$BaseUrl = "http://127.0.0.1:$Port"
$BinDir = Join-Path $InstallDir "bin"
$LogDir = Join-Path $InstallDir "logs"
$Exe = Join-Path $BinDir "memnest.exe"

if (-not $BinPath) {
  foreach ($candidate in ".\memnest.exe", "$Root\memnest.exe", ".\target\release\memnest.exe", "$Root\target\release\memnest.exe") {
    if (Test-Path $candidate) { $BinPath = $candidate; break }
  }
}
if (-not $BinPath -or -not (Test-Path $BinPath)) {
  throw "memnest.exe not found. Extract a release zip, build from source, or pass -BinPath C:\path\memnest.exe"
}
$BinPath = (Resolve-Path $BinPath).Path

# memnest.exe links the Visual C++ runtime dynamically. Starting it without the
# runtime opens a blocking error dialog, so check for the files instead.
$missing = @("vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll") | Where-Object {
  -not (Test-Path (Join-Path "$env:SystemRoot\System32" $_)) -and -not (Test-Path (Join-Path (Split-Path -Parent $BinPath) $_))
}
if ($missing) {
  throw "Microsoft Visual C++ Redistributable (x64) is required but $($missing -join ', ') is missing. Install https://aka.ms/vs/17/release/vc_redist.x64.exe and run this installer again."
}

function Stop-Memnest {
  # Stopping a task ends its console host but not the memnest.exe under it.
  foreach ($name in $TaskName, "$TaskName Watch") {
    if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
      Stop-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
    }
  }
  Get-Process -Name memnest -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $Exe } |
    Stop-Process -Force -ErrorAction SilentlyContinue
  for ($i = 0; $i -lt 50; $i++) {
    if (-not (Get-Process -Name memnest -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe })) { return }
    Start-Sleep -Milliseconds 200
  }
  throw "could not stop the running $Exe"
}

function Test-PortOpen {
  $client = New-Object System.Net.Sockets.TcpClient
  try {
    $pending = $client.BeginConnect("127.0.0.1", $Port, $null, $null)
    return ($pending.AsyncWaitHandle.WaitOne(1000) -and $client.Connected)
  }
  catch { return $false }
  finally { $client.Close() }
}

function Register-MemnestTask {
  param([string]$Name, [string]$Arguments, [string]$Launcher, [string]$LogName, [string]$Description)
  # A launcher file keeps every path quoted however the task runs it, and lets
  # cmd append the output to a log. NO_COLOR keeps escape codes out of the log.
  # The file is UTF-8 without a BOM and switches cmd to that code page first, so
  # a path outside the system code page (a Korean user name on an English
  # Windows) survives. cmd expands %, hence the doubling.
  $launcherPath = Join-Path $InstallDir $Launcher
  $lines = "@echo off", "chcp 65001 >nul", "set NO_COLOR=1", "`"$Exe`" $Arguments >> `"$LogDir\$LogName`" 2>&1"
  [System.IO.File]::WriteAllLines($launcherPath, [string[]]($lines | ForEach-Object { $_.Replace("%", "%%") }), (New-Object System.Text.UTF8Encoding $false))
  # conhost --headless runs the console program without a window.
  $user = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
  $action = New-ScheduledTaskAction -Execute "$env:SystemRoot\System32\conhost.exe" `
    -Argument "--headless `"$env:SystemRoot\System32\cmd.exe`" /d /c `"$launcherPath`"" -WorkingDirectory $InstallDir
  # The repeating trigger restarts a crashed process within five minutes; a
  # running instance makes it a no-op (the default policy ignores the new start).
  $triggers = @(
    (New-ScheduledTaskTrigger -AtLogOn -User $user),
    (New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 5))
  )
  $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
  $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable -ExecutionTimeLimit ([TimeSpan]::Zero)
  Register-ScheduledTask -TaskName $Name -Action $action -Trigger $triggers -Principal $principal -Settings $settings -Description $Description -Force | Out-Null
  Start-ScheduledTask -TaskName $Name
}

function Add-UserPathEntry {
  param([string]$Dir, [string]$Name = "Path")
  $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Environment", $true)
  try {
    # Read unexpanded and keep the value kind, so %VAR% entries survive.
    $current = [string]$key.GetValue($Name, "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $entries = @($current.Split(";") | Where-Object { $_ })
    if ($entries | Where-Object { $_.TrimEnd("\") -eq $Dir.TrimEnd("\") }) { return $false }
    $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
    if ($current) { $kind = $key.GetValueKind($Name) }
    $key.SetValue($Name, (($entries + $Dir) -join ";"), $kind)
  }
  finally { $key.Close() }
  # Deleting a variable that does not exist still broadcasts the environment
  # change, so new terminals pick up the entry without a logoff.
  [Environment]::SetEnvironmentVariable("MEMNEST_PATH_REFRESH", $null, "User")
  return $true
}

function Find-Python {
  # The Microsoft Store alias named python.exe exists without Python installed,
  # so a candidate counts only when it really runs.
  foreach ($candidate in @("py", "-3"), @("python"), @("python3")) {
    if (-not (Get-Command $candidate[0] -ErrorAction SilentlyContinue)) { continue }
    $probe = @($candidate | Select-Object -Skip 1) + @("-c", "import sys; sys.exit(0 if sys.version_info >= (3, 8) else 1)")
    try {
      & $candidate[0] @probe 2>$null | Out-Null
      if ($LASTEXITCODE -eq 0) { return $candidate }
    }
    catch { }
  }
  return $null
}

function Invoke-MemnestJson {
  param([string]$Path, [hashtable]$Body)
  $bytes = [System.Text.Encoding]::UTF8.GetBytes(($Body | ConvertTo-Json -Compress))
  Invoke-RestMethod -Method Post -Uri "$BaseUrl$Path" -ContentType "application/json" -Body $bytes -TimeoutSec 600
}

Stop-Memnest
if (Test-PortOpen) {
  throw "port $Port on 127.0.0.1 is already in use. A memnest running inside WSL shares localhost with Windows, so the two cannot both use port $Port; stop the other service or pass -Port <free port>."
}

New-Item -ItemType Directory -Force -Path $BinDir, $LogDir, (Join-Path $InstallDir "scripts"), $DataDir | Out-Null
Copy-Item $BinPath $Exe -Force
foreach ($script in "uninstall-windows.ps1", "validate-installed-windows.ps1", "support-bundle-windows.ps1", "setup-clients.py") {
  Copy-Item (Join-Path $ScriptDir $script) (Join-Path $InstallDir "scripts") -Force
}

$watchArguments = "--data-dir `"$DataDir`" watch --url $BaseUrl"
foreach ($path in $WatchPath) { $watchArguments += " --path `"$path`"" }
Register-MemnestTask -Name $TaskName -Launcher "memnest-server.cmd" -LogName "server.log" -Description "Memnest memory server (per-user, 127.0.0.1:$Port)" `
  -Arguments "--host 127.0.0.1 --port $Port --data-dir `"$DataDir`""
Register-MemnestTask -Name "$TaskName Watch" -Launcher "memnest-watch.cmd" -LogName "watch.log" -Description "Memnest conversation watcher" `
  -Arguments $watchArguments

for ($i = 0; $i -lt 60; $i++) {
  try {
    Invoke-WebRequest -UseBasicParsing "$BaseUrl/health" | Out-Null
    Write-Host "Health check passed: $BaseUrl/health"
    break
  }
  catch {
    if ($i -eq 59) { throw "Memnest did not answer $BaseUrl/health. See $LogDir\server.log" }
    Start-Sleep -Seconds 1
  }
}

if (-not $NoPath) {
  if (Add-UserPathEntry -Dir $BinDir) { Write-Host "Added $BinDir to your user PATH. Open a new terminal to use the memnest command." }
}

if (-not $NoClients) {
  $python = Find-Python
  if ($python) {
    $clientArguments = @($python | Select-Object -Skip 1) + @((Join-Path $InstallDir "scripts\setup-clients.py"), "--bin", $Exe, "--url", $BaseUrl)
    if ($Autocontext) { $clientArguments += "--autocontext" }
    & $python[0] @clientArguments
    if ($LASTEXITCODE -ne 0) { throw "client setup failed; the server and watcher are installed and running" }
  }
  else {
    Write-Host "Python 3 was not found, so no client was configured. Either install Python and run"
    Write-Host "  python `"$InstallDir\scripts\setup-clients.py`" --bin `"$Exe`" --url $BaseUrl"
    Write-Host "or connect a client by hand:"
    Write-Host "  MCP endpoint (Claude Code, Codex, Cursor, others): $BaseUrl/mcp"
    Write-Host "  Task card hook for SessionStart and UserPromptSubmit: `"$($Exe.Replace('\', '/'))`" hook --url $BaseUrl --no-recall"
  }
}

if (-not $NoVerify) {
  Write-Host "Verifying a scratch remember/search round trip. The first one downloads the embedding model (about 1.1 GB)."
  $marker = "memnest-setup-roundtrip-$([guid]::NewGuid().ToString('N'))"
  $created = Invoke-MemnestJson "/add" @{ text = $marker; project = "memnest-setup-check" }
  $found = Invoke-MemnestJson "/search" @{ query = $marker; project = "memnest-setup-check"; n_results = 5 }
  if (-not (($found | ConvertTo-Json -Depth 8) -match $marker)) { throw "scratch remember/search verification failed" }
  if ($created.id) { Invoke-MemnestJson "/delete" @{ ids = @($created.id) } | Out-Null }
  Write-Host "setup_roundtrip_ok"
}

Write-Host "Setup complete. MCP endpoint: $BaseUrl/mcp"
Write-Host "Scheduled tasks: '$TaskName' (server) and '$TaskName Watch' (conversation capture). Logs: $LogDir"
Write-Host "Uninstall: powershell -ExecutionPolicy Bypass -File `"$InstallDir\scripts\uninstall-windows.ps1`""
Write-Host "Data in $DataDir is retained unless -RemoveData is passed to the uninstaller."
