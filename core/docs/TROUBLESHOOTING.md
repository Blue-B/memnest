# Troubleshooting

## Dashboard does not open

Check health:

```bash
curl -fsS http://127.0.0.1:3111/health
```

Linux user service:

```bash
systemctl --user status memnest.service --no-pager -l
journalctl --user -u memnest.service -n 100 --no-pager
```

Linux system service:

```bash
systemctl status memnest.service --no-pager -l
journalctl -u memnest.service -n 100 --no-pager
```

Windows service:

```powershell
Get-Service memnest
Get-Content "$env:ProgramData\Memnest\logs\*.log" -Tail 100
```

Create a support bundle:

```bash
scripts/support-bundle.sh --user
```

```powershell
.\scripts\support-bundle-windows.ps1
```

## Port 3111 is already in use

Run Memnest on another port:

```bash
MEMNEST_PORT=3211 scripts/install-linux.sh --user --bin target/release/memnest
```

Then open `http://127.0.0.1:3211/`.

## WSL service is not running after reboot

Check the Windows scheduled task:

```powershell
Get-ScheduledTask -TaskName "Memnest WSL"
Start-ScheduledTask -TaskName "Memnest WSL"
```

Then check inside WSL:

```powershell
wsl -d Ubuntu-24.04 -- systemctl --user status memnest.service
```

## Remote bind fails

Binding to `0.0.0.0` requires a token:

```bash
MEMNEST_TOKEN='replace-with-a-secret' memnest --host 0.0.0.0
```

Call the API with:

```bash
curl -H "Authorization: Bearer replace-with-a-secret" http://127.0.0.1:3111/health
```

## First search or save fails offline

Memnest uses a local embedding model. The first operation that needs embeddings
downloads the model into the configured data directory. On machines that must run
offline, warm the cache once while online:

```bash
memnest --data-dir ~/.memnest --warmup-embedding
memnest --data-dir ~/.memnest --doctor
```

On Windows native installs:

```powershell
& "$env:ProgramData\Memnest\app\memnest.exe" --data-dir "$env:ProgramData\Memnest\data" --warmup-embedding
& "$env:ProgramData\Memnest\app\memnest.exe" --data-dir "$env:ProgramData\Memnest\data" --doctor
```

## Backup before upgrade

Stop the service first:

```bash
systemctl --user stop memnest.service
memnest --data-dir ~/.memnest --backup-dir ~/memnest-backup
systemctl --user start memnest.service
```

## Restore

Stop the service and restore:

```bash
systemctl --user stop memnest.service
memnest --data-dir ~/.memnest --restore-dir ~/memnest-backup --force
systemctl --user start memnest.service
```
