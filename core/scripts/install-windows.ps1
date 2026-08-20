param(
  [string]$InstallDir = "$env:ProgramData\Memnest\app",
  [string]$DataDir = "$env:ProgramData\Memnest\data",
  [string]$ServiceName = "memnest",
  [string]$HostAddress = "127.0.0.1",
  [int]$Port = 3111,
  [string]$WinSWVersion = "v2.12.0",
  [string]$BinPath = "",
  [string]$WinSWPath = "",
  [string]$WinSWSha256 = ""
)

$ErrorActionPreference = "Stop"

# The service wrapper runs elevated, so its bytes are pinned. This is the
# SHA-256 of WinSW-x64.exe from the winsw/winsw release tagged $PinnedWinSWVersion.
# Bump both values together, never one alone.
$PinnedWinSWVersion = "v2.12.0"
$PinnedWinSWSha256 = "05b82d46ad331cc16bdc00de5c6332c1ef818df8ceefcd49c726553209b3a0da"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $ScriptDir
$LocalHosts = @("127.0.0.1", "localhost", "::1")
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).
  IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
  throw "install-windows.ps1 must run from an elevated PowerShell prompt."
}
if ($LocalHosts -notcontains $HostAddress) {
  throw "install-windows.ps1 only supports local service binds. Use 127.0.0.1 for packaged installs; configure remote access manually with MEMNEST_TOKEN and a reviewed network policy."
}

if (-not $BinPath) {
  if (Test-Path ".\memnest.exe") {
    $BinPath = ".\memnest.exe"
  }
  elseif (Test-Path "$Root\memnest.exe") {
    $BinPath = "$Root\memnest.exe"
  }
  elseif (Test-Path ".\target\release\memnest.exe") {
    $BinPath = ".\target\release\memnest.exe"
  }
  elseif (Test-Path "$Root\target\release\memnest.exe") {
    $BinPath = "$Root\target\release\memnest.exe"
  }
  elseif (Test-Path "$Root\Cargo.toml") {
    Push-Location $Root
    cargo build --release
    Pop-Location
    $BinPath = "$Root\target\release\memnest.exe"
  }
}

if (-not $BinPath -or -not (Test-Path $BinPath)) {
  throw "memnest.exe not found. Extract a Windows release archive, build from source, or pass -BinPath C:\path\memnest.exe"
}

if (-not $WinSWPath -and (Test-Path ".\WinSW-x64.exe")) {
  $WinSWPath = ".\WinSW-x64.exe"
}
elseif (-not $WinSWPath -and (Test-Path "$Root\WinSW-x64.exe")) {
  $WinSWPath = "$Root\WinSW-x64.exe"
}

if (-not $WinSWSha256 -and (Test-Path ".\WinSW-x64.exe.sha256")) {
  $WinSWSha256 = (Get-Content ".\WinSW-x64.exe.sha256" -Raw).Trim().Split(" ")[0]
}
elseif (-not $WinSWSha256 -and (Test-Path "$Root\WinSW-x64.exe.sha256")) {
  $WinSWSha256 = (Get-Content "$Root\WinSW-x64.exe.sha256" -Raw).Trim().Split(" ")[0]
}

if (-not $WinSWSha256) {
  if ($WinSWVersion -eq $PinnedWinSWVersion) {
    $WinSWSha256 = $PinnedWinSWSha256
  }
  else {
    throw "no SHA-256 to verify WinSW against. This script only pins $PinnedWinSWVersion; pass -WinSWSha256 <hash> together with -WinSWVersion $WinSWVersion, or drop a WinSW-x64.exe.sha256 next to the wrapper."
  }
}

$LogDir = "$env:ProgramData\Memnest\logs"
New-Item -ItemType Directory -Force -Path $InstallDir, $DataDir, $LogDir | Out-Null
Copy-Item $BinPath "$InstallDir\memnest.exe" -Force
if (Test-Path "$Root\static") {
  New-Item -ItemType Directory -Force -Path "$InstallDir\static" | Out-Null
  Copy-Item "$Root\static\*" -Destination "$InstallDir\static" -Recurse -Force
}
else {
  throw "dashboard static assets not found: $Root\static"
}

$winsw = "$InstallDir\memnest-service.exe"
$escapedDataDir = [System.Security.SecurityElement]::Escape($DataDir)
$escapedLogDir = [System.Security.SecurityElement]::Escape($LogDir)
$escapedServiceName = [System.Security.SecurityElement]::Escape($ServiceName)
$escapedHost = [System.Security.SecurityElement]::Escape($HostAddress)
$escapedPort = [System.Security.SecurityElement]::Escape($Port.ToString())
$xmlTemplate = "$Root\packaging\windows\memnest-service.xml"
if (-not (Test-Path $xmlTemplate)) {
  throw "service template not found: $xmlTemplate"
}
$xml = Get-Content $xmlTemplate -Raw
$xml = $xml.Replace("<id>memnest</id>", "<id>$escapedServiceName</id>")
$xml = $xml.Replace("%BASE%\..\data", $escapedDataDir)
$xml = $xml.Replace("%BASE%\..\logs", $escapedLogDir)
$xml = $xml.Replace("--host 127.0.0.1", "--host $escapedHost")
$xml = $xml.Replace("--port 3111", "--port $escapedPort")
Set-Content -Path "$InstallDir\memnest-service.xml" -Value $xml -Encoding UTF8
if (-not (Test-Path $winsw)) {
  if ($WinSWPath) {
    if (-not (Test-Path $WinSWPath)) {
      throw "WinSW wrapper not found: $WinSWPath"
    }
    Copy-Item $WinSWPath $winsw -Force
  }
  else {
    $url = "https://github.com/winsw/winsw/releases/download/$WinSWVersion/WinSW-x64.exe"
    Invoke-WebRequest -Uri $url -OutFile $winsw
  }
}

$actualHash = (Get-FileHash $winsw -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $WinSWSha256.ToLowerInvariant()) {
  Remove-Item $winsw -Force -ErrorAction SilentlyContinue
  throw "WinSW SHA-256 mismatch. Expected $WinSWSha256 but got $actualHash. The wrapper was removed; nothing was installed."
}

Push-Location $InstallDir
try {
  & $winsw stop 2>$null | Out-Null
  & $winsw uninstall 2>$null | Out-Null
  & $winsw install
  & $winsw start
}
finally {
  Pop-Location
}

$health = "http://${HostAddress}:$Port/health"
for ($i = 0; $i -lt 30; $i++) {
  try {
    Invoke-WebRequest -UseBasicParsing $health | Out-Null
    Write-Host "Health check passed: $health"
    break
  }
  catch {
    if ($i -eq 29) { throw "Memnest did not answer health check at $health" }
    Start-Sleep -Seconds 1
  }
}

Write-Host "Installed Windows service '$ServiceName'."
Write-Host "Dashboard: http://127.0.0.1:$Port/"
