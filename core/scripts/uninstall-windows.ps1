param(
  [string]$InstallDir = "$env:LOCALAPPDATA\memnest",
  [string]$DataDir = "$env:USERPROFILE\.memnest",
  [string]$TaskName = "Memnest",
  [switch]$RemoveData
)

# Removes the per-user install made by install-windows.ps1. No elevation.
# Keep this file ASCII (see install-windows.ps1).
$ErrorActionPreference = "Stop"
$BinDir = Join-Path $InstallDir "bin"
$Exe = Join-Path $BinDir "memnest.exe"

foreach ($name in "$TaskName Watch", $TaskName) {
  if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
    Stop-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $name -Confirm:$false
  }
}
# Stopping a task ends its console host but not the memnest.exe under it.
Get-Process -Name memnest -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -eq $Exe } |
  Stop-Process -Force -ErrorAction SilentlyContinue
for ($i = 0; $i -lt 50; $i++) {
  if (-not (Get-Process -Name memnest -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe })) { break }
  Start-Sleep -Milliseconds 200
}

$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Environment", $true)
try {
  $current = [string]$key.GetValue("Path", "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
  $entries = @($current.Split(";") | Where-Object { $_ })
  $kept = @($entries | Where-Object { $_.TrimEnd("\") -ne $BinDir.TrimEnd("\") })
  if ($kept.Count -ne $entries.Count) {
    $key.SetValue("Path", ($kept -join ";"), $key.GetValueKind("Path"))
    [Environment]::SetEnvironmentVariable("MEMNEST_PATH_REFRESH", $null, "User")
  }
}
finally { $key.Close() }

# Only what the installer put there: -InstallDir may be a folder that holds
# other things. This script normally runs from $InstallDir\scripts; PowerShell
# has already read it, so that directory can go.
foreach ($own in "bin\memnest.exe", "logs", "scripts", "memnest-server.cmd", "memnest-watch.cmd") {
  $path = Join-Path $InstallDir $own
  if (Test-Path $path) { Remove-Item -Recurse -Force $path }
}
foreach ($dir in $BinDir, $InstallDir) {
  if ((Test-Path $dir) -and -not (Get-ChildItem -Force $dir)) { Remove-Item -Force $dir }
}
if ($RemoveData) {
  if (Test-Path $DataDir) { Remove-Item -Recurse -Force $DataDir }
  $lock = Join-Path (Split-Path -Parent $DataDir) ".$(Split-Path -Leaf $DataDir).writer.lock"
  if (Test-Path $lock) { Remove-Item -Force $lock }
}

# Releases before the per-user installer registered an elevated WinSW service.
if (Test-Path "$env:ProgramData\Memnest") {
  Write-Warning "An older service install remains in $env:ProgramData\Memnest. From an elevated prompt run '$env:ProgramData\Memnest\app\memnest-service.exe stop' and 'uninstall', then delete that folder. To keep its memories, first run: memnest --data-dir `"$env:ProgramData\Memnest\data`" --backup-dir <folder>, then memnest --restore-dir <folder>."
}

Write-Host "Memnest uninstalled."
if (-not $RemoveData) { Write-Host "Data was kept in $DataDir." }
