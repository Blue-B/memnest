param(
  [string]$InstallDir = "$env:ProgramData\Memnest\app",
  [switch]$RemoveData
)

$ErrorActionPreference = "Stop"
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).
  IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
  throw "uninstall-windows.ps1 must run from an elevated PowerShell prompt."
}

$winsw = "$InstallDir\memnest-service.exe"
if (Test-Path $winsw) {
  Push-Location $InstallDir
  try {
    & $winsw stop 2>$null | Out-Null
    & $winsw uninstall 2>$null | Out-Null
  }
  finally {
    Pop-Location
  }
}

Remove-Item -Recurse -Force $InstallDir -ErrorAction SilentlyContinue
if ($RemoveData) {
  Remove-Item -Recurse -Force "$env:ProgramData\Memnest" -ErrorAction SilentlyContinue
}

Write-Host "Memnest uninstalled."
