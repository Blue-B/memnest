param(
  [string]$Distro = "Ubuntu-24.04",
  [string]$RepoPath = "/home/<your-wsl-username>/palimpsest",
  [string]$TaskName = "Palimpsest WSL",
  [int]$Port = 3111,
  [switch]$RegisterTaskOnly
)

$ErrorActionPreference = "Stop"

if (-not $RegisterTaskOnly) {
  Write-Host "Installing Palimpsest inside WSL distro '$Distro'..."
  wsl -d $Distro -- bash -lc "cd '$RepoPath' && cargo build --release && scripts/install-linux.sh --user --bin target/release/palimpsest"
}
else {
  Write-Host "Skipping Linux install. Registering the Windows logon task for existing WSL service."
  wsl -d $Distro -- bash -lc "systemctl --user is-enabled palimpsest.service >/dev/null"
}

$action = New-ScheduledTaskAction -Execute "wsl.exe" -Argument "-d $Distro -- bash -lc `"systemctl --user start palimpsest.service`""
$trigger = New-ScheduledTaskTrigger -AtLogOn
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings -Description "Start Palimpsest WSL service at logon" -Force | Out-Null

Write-Host "Installed scheduled task '$TaskName'."
$health = "http://127.0.0.1:$Port/health"
for ($i = 0; $i -lt 30; $i++) {
  try {
    Invoke-WebRequest -UseBasicParsing $health | Out-Null
    Write-Host "Health check passed: $health"
    break
  }
  catch {
    if ($i -eq 29) { throw "Palimpsest did not answer health check at $health" }
    Start-Sleep -Seconds 1
  }
}
Write-Host "Dashboard: http://127.0.0.1:$Port/"
