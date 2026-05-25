param(
  [string]$Distro = "Ubuntu-24.04",
  [string]$RepoPath = "/home/<your-wsl-username>/palimpsest",
  [string]$TaskName = "Palimpsest WSL",
  [switch]$RemoveData
)

$ErrorActionPreference = "Stop"

Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue

$removeDataFlag = if ($RemoveData) { "--remove-data" } else { "" }
wsl -d $Distro -- bash -lc "if [ -x '$RepoPath/scripts/uninstall-linux.sh' ]; then '$RepoPath/scripts/uninstall-linux.sh' --user $removeDataFlag; else systemctl --user disable --now palimpsest.service 2>/dev/null || true; rm -f ~/.config/systemd/user/palimpsest.service ~/.local/bin/palimpsest; rm -rf ~/.local/share/palimpsest; systemctl --user daemon-reload; fi"

Write-Host "Palimpsest WSL integration uninstalled."
