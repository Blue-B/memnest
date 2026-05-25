# Troubleshooting

## Dashboard does not open

Check health:

```bash
curl -fsS http://127.0.0.1:3111/health
```

Linux user service:

```bash
systemctl --user status palimpsest.service --no-pager -l
journalctl --user -u palimpsest.service -n 100 --no-pager
```

Linux system service:

```bash
systemctl status palimpsest.service --no-pager -l
journalctl -u palimpsest.service -n 100 --no-pager
```

Windows service:

```powershell
Get-Service palimpsest
Get-Content "$env:ProgramData\Palimpsest\logs\*.log" -Tail 100
```

Create a support bundle:

```bash
scripts/support-bundle.sh --user
```

```powershell
.\scripts\support-bundle-windows.ps1
```

## Port 3111 is already in use

Run Palimpsest on another port:

```bash
PALIMPSEST_PORT=3211 scripts/install-linux.sh --user --bin target/release/palimpsest
```

Then open `http://127.0.0.1:3211/`.

## WSL service is not running after reboot

Check the Windows scheduled task:

```powershell
Get-ScheduledTask -TaskName "Palimpsest WSL"
Start-ScheduledTask -TaskName "Palimpsest WSL"
```

Then check inside WSL:

```powershell
wsl -d Ubuntu-24.04 -- systemctl --user status palimpsest.service
```

## Remote bind fails

Binding to `0.0.0.0` requires a token:

```bash
PALIMPSEST_TOKEN='replace-with-a-secret' palimpsest --host 0.0.0.0
```

Call the API with:

```bash
curl -H "Authorization: Bearer replace-with-a-secret" http://127.0.0.1:3111/health
```

## First search or save fails offline

Palimpsest uses a local embedding model. The first operation that needs embeddings
downloads the model into the configured data directory. On machines that must run
offline, warm the cache once while online:

```bash
palimpsest --data-dir ~/.palimpsest --warmup-embedding
palimpsest --data-dir ~/.palimpsest --doctor
```

On Windows native installs:

```powershell
& "$env:ProgramData\Palimpsest\app\palimpsest.exe" --data-dir "$env:ProgramData\Palimpsest\data" --warmup-embedding
& "$env:ProgramData\Palimpsest\app\palimpsest.exe" --data-dir "$env:ProgramData\Palimpsest\data" --doctor
```

## Backup before upgrade

Stop the service first:

```bash
systemctl --user stop palimpsest.service
palimpsest --data-dir ~/.palimpsest --backup-dir ~/palimpsest-backup
systemctl --user start palimpsest.service
```

## Restore

Stop the service and restore:

```bash
systemctl --user stop palimpsest.service
palimpsest --data-dir ~/.palimpsest --restore-dir ~/palimpsest-backup --force
systemctl --user start palimpsest.service
```
