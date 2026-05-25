# Updating Palimpsest

The supported update channel is installer-managed upgrade: install the new release over the existing service. Data remains in the configured data directory.

## Linux

Back up first:

```bash
systemctl --user stop palimpsest.service
palimpsest --data-dir ~/.palimpsest --backup-dir ~/palimpsest-backup
```

Install the new release:

```bash
VERSION=v0.1.0 scripts/install.sh --user
```

Verify:

```bash
curl -fsS http://127.0.0.1:3111/health
systemctl --user status palimpsest.service --no-pager -l
```

## WSL

Run from Windows PowerShell:

```powershell
.\scripts\install-wsl.ps1 -Distro Ubuntu-24.04 -RepoPath /home/<your-wsl-username>/palimpsest
```

## Windows native

Run from an elevated PowerShell prompt:

```powershell
.\scripts\install-windows.ps1
```

The installer stops and replaces the existing service wrapper, then starts the service again.

## Rollback

Restore from backup:

```bash
systemctl --user stop palimpsest.service
palimpsest --data-dir ~/.palimpsest --restore-dir ~/palimpsest-backup --force
systemctl --user start palimpsest.service
```
