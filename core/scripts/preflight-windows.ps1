param(
  [string]$BinPath = "",
  [string]$InstallDir = "$env:LOCALAPPDATA\memnest",
  [string]$DataDir = "$env:USERPROFILE\.memnest",
  [int]$Port = 3111
)

# Read-only checks before install-windows.ps1. No elevation.
# Keep this file ASCII (see install-windows.ps1).
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $ScriptDir
$failures = 0

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
    Write-Host "fail: $Name ($($_.Exception.Message))"
    $script:failures += 1
  }
}

function Test-WritableTarget {
  param([string]$Path)

  $target = $Path
  while ($target -and -not (Test-Path $target)) {
    $target = Split-Path -Parent $target
  }
  if (-not $target) {
    throw "no existing parent path for $Path"
  }

  $probe = Join-Path $target ".memnest-preflight-$([guid]::NewGuid().ToString('N')).tmp"
  New-Item -ItemType File -Path $probe -Force | Out-Null
  Remove-Item $probe -Force
}

if (-not $BinPath) {
  foreach ($candidate in ".\memnest.exe", "$Root\memnest.exe", ".\target\release\memnest.exe", "$Root\target\release\memnest.exe") {
    if (Test-Path $candidate) { $BinPath = $candidate; break }
  }
}

Check-Ok "64-bit x86 Windows" { if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64") { throw "found $env:PROCESSOR_ARCHITECTURE" } }
Check-Ok "memnest.exe is available" { if (-not $BinPath -or -not (Test-Path $BinPath)) { throw "pass -BinPath" } }
Check-Ok "Microsoft Visual C++ runtime (x64) is installed" {
  foreach ($dll in "vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll") {
    if (-not (Test-Path (Join-Path "$env:SystemRoot\System32" $dll))) { throw "$dll is missing; install https://aka.ms/vs/17/release/vc_redist.x64.exe" }
  }
}
Check-Ok "scheduled tasks are available" { Get-Command Register-ScheduledTask -ErrorAction Stop }
Check-Ok "install directory or nearest parent is writable" { Test-WritableTarget -Path $InstallDir }
Check-Ok "data directory or nearest parent is writable" { Test-WritableTarget -Path $DataDir }

$portInUse = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
if ($portInUse) {
  Write-Warning "port $Port is already listening. An installed Memnest is expected here; anything else (for example Memnest inside WSL) needs -Port <free port>."
}
else {
  Write-Host "ok: port $Port is not currently listening"
}

if ($failures -gt 0) {
  throw "preflight failed with $failures issue(s)"
}

Write-Host "preflight_windows_ok"
