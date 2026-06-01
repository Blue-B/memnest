param(
  [string]$BinPath = "",
  [string]$InstallDir = "$env:ProgramData\Memnest\app",
  [string]$DataDir = "$env:ProgramData\Memnest\data",
  [string]$HostAddress = "127.0.0.1",
  [int]$Port = 3111
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $ScriptDir
$failures = 0
$LocalHosts = @("127.0.0.1", "localhost", "::1")

function Check-Ok {
  param(
    [string]$Name,
    [scriptblock]$Command
  )
  try {
    & $Command | Out-Null
    Write-Host "ok: $Name"
  }
  catch {
    Write-Error "fail: $Name"
    $script:failures += 1
  }
}

function Test-WritableTarget {
  param([string]$Path)

  if (Test-Path $Path) {
    $target = $Path
  }
  else {
    $target = Split-Path -Parent $Path
    while ($target -and -not (Test-Path $target)) {
      $target = Split-Path -Parent $target
    }
  }

  if (-not $target) {
    throw "no existing parent path for $Path"
  }

  $probe = Join-Path $target ".memnest-preflight-$([guid]::NewGuid().ToString('N')).tmp"
  New-Item -ItemType File -Path $probe -Force | Out-Null
  Remove-Item $probe -Force
}

$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).
  IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($isAdmin) {
  Write-Host "ok: elevated PowerShell"
}
else {
  Write-Error "fail: elevated PowerShell"
  $failures += 1
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
}

Check-Ok "memnest.exe is available" { if (-not $BinPath -or -not (Test-Path $BinPath)) { throw "missing binary" } }
Check-Ok "Windows service template is available" { if (-not (Test-Path "$Root\packaging\windows\memnest-service.xml")) { throw "missing service template" } }
Check-Ok "dashboard static assets are available" { if (-not (Test-Path "$Root\static\memory-atlas.png")) { throw "missing dashboard static assets" } }
Check-Ok "install directory or nearest parent is writable" { Test-WritableTarget -Path $InstallDir }
Check-Ok "data directory or nearest parent is writable" { Test-WritableTarget -Path $DataDir }
Check-Ok "host is a supported local bind" { if ($LocalHosts -notcontains $HostAddress) { throw "unsupported host bind: $HostAddress" } }

$portInUse = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
if ($portInUse) {
  Write-Warning "port $Port is already listening on $HostAddress"
}
else {
  Write-Host "ok: port $Port is not currently listening on $HostAddress"
}

if ($failures -gt 0) {
  throw "preflight failed with $failures issue(s)"
}

Write-Host "preflight_windows_ok"
