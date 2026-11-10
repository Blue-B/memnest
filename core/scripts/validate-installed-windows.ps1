param(
  [string]$InstallDir = "$env:LOCALAPPDATA\memnest",
  [string]$DataDir = "$env:USERPROFILE\.memnest",
  [string]$TaskName = "Memnest",
  [int]$Port = 3111
)

# Validates the per-user install: both scheduled tasks, health, doctor, and
# recovery after the server task is restarted. No elevation.
# Keep this file ASCII (see install-windows.ps1).
$ErrorActionPreference = "Stop"
$Exe = Join-Path $InstallDir "bin\memnest.exe"
$health = "http://127.0.0.1:$Port/health"

function Wait-MemnestHealth {
  for ($i = 0; $i -lt 30; $i++) {
    try {
      Invoke-WebRequest -UseBasicParsing $health | Out-Null
      return
    }
    catch {
      if ($i -eq 29) { throw "health check failed at $health" }
      Start-Sleep -Seconds 1
    }
  }
}

function Get-InstalledMemnest {
  param([string]$Pattern)
  Get-CimInstance Win32_Process -Filter "Name='memnest.exe'" |
    Where-Object { $_.ExecutablePath -eq $Exe -and $_.CommandLine -like $Pattern }
}

foreach ($name in $TaskName, "$TaskName Watch") {
  $task = Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
  if (-not $task) { throw "scheduled task '$name' is not registered" }
  if ($task.State -ne "Running") { throw "scheduled task '$name' is not running: $($task.State)" }
}
if (-not (Get-InstalledMemnest "* watch *")) { throw "no memnest watch process is running from $Exe" }

Wait-MemnestHealth
& $Exe --data-dir $DataDir --doctor
if ($LASTEXITCODE -ne 0) { throw "memnest --doctor reported errors" }

# Stopping a task ends its console host but not the memnest.exe under it.
Stop-ScheduledTask -TaskName $TaskName
Get-InstalledMemnest "*--port $Port *" | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
Start-Sleep -Seconds 1
Start-ScheduledTask -TaskName $TaskName
Wait-MemnestHealth
& $Exe --port $Port --data-dir $DataDir status
Write-Host "validate_installed_windows_ok"
