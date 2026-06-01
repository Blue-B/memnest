param(
  [string]$ServiceName = "memnest",
  [int]$Port = 3111,
  [string]$InstallDir = "$env:ProgramData\Memnest\app",
  [string]$DataDir = "$env:ProgramData\Memnest\data"
)

$ErrorActionPreference = "Stop"
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).
  IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
  throw "validate-installed-windows.ps1 must run from an elevated PowerShell prompt."
}

function Wait-MemnestHealth {
  param([string]$Url)

  for ($i = 0; $i -lt 30; $i++) {
    try {
      Invoke-WebRequest -UseBasicParsing $Url | Out-Null
      return
    }
    catch {
      if ($i -eq 29) {
        throw "health check failed at $Url"
      }
      Start-Sleep -Seconds 1
    }
  }
}

$health = "http://127.0.0.1:$Port/health"
$service = Get-Service -Name $ServiceName
if ($service.Status -ne "Running") {
  throw "service '$ServiceName' is not running: $($service.Status)"
}

Wait-MemnestHealth -Url $health
Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$Port/assets/memory-atlas.png" | Out-Null
& "$InstallDir\memnest.exe" --data-dir $DataDir --doctor
Restart-Service -Name $ServiceName -Force
Wait-MemnestHealth -Url $health
Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$Port/assets/memory-atlas.png" | Out-Null
Write-Host "validate_installed_windows_ok"
