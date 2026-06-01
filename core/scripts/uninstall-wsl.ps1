param(
  [string]$Distro = "Ubuntu-24.04",
  [string]$RepoPath = "/home/<your-wsl-username>/memnest",
  [string]$TaskName = "Memnest WSL",
  [switch]$RemoveData
)

$ErrorActionPreference = "Stop"

Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue

$removeDataFlag = if ($RemoveData) { "--remove-data" } else { "" }
wsl -d $Distro -- bash -lc "if [ -x '$RepoPath/scripts/uninstall-linux.sh' ]; then '$RepoPath/scripts/uninstall-linux.sh' --user $removeDataFlag; else systemctl --user disable --now memnest.service 2>/dev/null || true; rm -f ~/.config/systemd/user/memnest.service ~/.local/bin/memnest; rm -rf ~/.local/share/memnest; systemctl --user daemon-reload; fi"

Write-Host "Memnest WSL integration uninstalled."
