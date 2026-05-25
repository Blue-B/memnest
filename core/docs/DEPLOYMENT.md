# Palimpsest deployment

This document defines the supported local-product deployment targets.

## Targets

- Linux native: systemd user service for developer machines, system service for servers.
- WSL: systemd user service inside the distro plus a Windows Scheduled Task that wakes WSL at logon.
- Windows native: `palimpsest.exe` wrapped by WinSW and installed as an automatic Windows service.

All packaged service installers bind to `127.0.0.1:3111` by default and intentionally support only local binds. Do not expose the server on a public interface without adding authentication in front of it.

The runtime data directory is explicit in every supported service installer:

- Linux user service: `~/.palimpsest`
- Linux system service: `/var/lib/palimpsest`
- WSL installer: the Linux user service path inside the selected distro
- Windows native service: `%ProgramData%\Palimpsest\data`

For manual runs, pass `--data-dir` or set `PALIMPSEST_DATA_DIR` to avoid mixing development and production memory stores.

## Linux native

Build and install for the current user:

```bash
cargo build --release
scripts/preflight-linux.sh --user --bin target/release/palimpsest
scripts/install-linux.sh --user --bin target/release/palimpsest
```

Install as a system service:

```bash
cargo build --release
scripts/install-linux.sh --system --bin target/release/palimpsest
```

Verify:

```bash
curl -fsS http://127.0.0.1:3111/health
systemctl --user status palimpsest.service
scripts/validate-installed.sh --user
~/.local/bin/palimpsest --data-dir ~/.palimpsest --warmup-embedding
```

For system mode, use `systemctl status palimpsest.service` and `scripts/validate-installed.sh --system`.
Run `/usr/local/bin/palimpsest --data-dir /var/lib/palimpsest --warmup-embedding` once on an online system if the machine must work offline later.

Uninstall:

```bash
scripts/uninstall-linux.sh --user
scripts/uninstall-linux.sh --system
```

## WSL

Run from Windows PowerShell:

```powershell
.\scripts\install-wsl.ps1 -Distro Ubuntu-24.04 -RepoPath /home/<your-wsl-username>/palimpsest
```

This installs the service inside WSL and registers a Windows logon task that starts it again after reboot.

For a product install from a Linux release archive, run the Linux installer inside WSL first:

```bash
scripts/install-linux.sh --user --bin ./palimpsest
```

Then register only the Windows wake task from PowerShell:

```powershell
.\scripts\install-wsl.ps1 -Distro Ubuntu-24.04 -RegisterTaskOnly
```

Verify:

```powershell
wsl -d Ubuntu-24.04 -- systemctl --user status palimpsest.service
Invoke-WebRequest http://127.0.0.1:3111/health
wsl -d Ubuntu-24.04 -- bash -lc "cd /home/<your-wsl-username>/palimpsest && scripts/validate-installed.sh --user"
wsl -d Ubuntu-24.04 -- bash -lc "$HOME/.local/bin/palimpsest --data-dir $HOME/.palimpsest --warmup-embedding"
```

Uninstall:

```powershell
.\scripts\uninstall-wsl.ps1 -Distro Ubuntu-24.04 -RepoPath /home/<your-wsl-username>/palimpsest
```

## Windows native

Run from an elevated PowerShell prompt. The installer, validator, and uninstaller fail early when PowerShell is not elevated because Windows service registration requires administrator rights:

```powershell
.\scripts\preflight-windows.ps1
.\scripts\install-windows.ps1
```

This accepts a release archive with `palimpsest.exe` in the current directory. From a source checkout it builds `target\release\palimpsest.exe` when needed. To use a custom binary:

```powershell
.\scripts\install-windows.ps1 -BinPath C:\path\to\palimpsest.exe
```

The release archive includes `WinSW-x64.exe` and `WinSW-x64.exe.sha256`; the installer uses them automatically when present. For controlled enterprise installs, provide a different pre-approved WinSW wrapper and expected hash:

```powershell
.\scripts\install-windows.ps1 -WinSWPath C:\path\WinSW-x64.exe -WinSWSha256 "<sha256>"
```

The installer copies `palimpsest.exe` under `%ProgramData%\Palimpsest\app` and registers it as an automatic service through WinSW.

To use another local port:

```powershell
.\scripts\install-windows.ps1 -Port 3211
```

Verify:

```powershell
Get-Service palimpsest
Invoke-WebRequest http://127.0.0.1:3111/health
.\scripts\validate-installed-windows.ps1
& "$env:ProgramData\Palimpsest\app\palimpsest.exe" --data-dir "$env:ProgramData\Palimpsest\data" --warmup-embedding
```

Uninstall:

```powershell
.\scripts\uninstall-windows.ps1
```

## Release artifacts

Release archives should include:

- `palimpsest` or `palimpsest.exe`
- `scripts/`
- `packaging/`
- `docs/DEPLOYMENT.md`
- `static/memory-atlas.png`

Installers copy application assets into the service working directory. Uninstallers
remove the application binary and assets; runtime memory data is kept unless
`--remove-data` or `-RemoveData` is explicitly passed.

## Product readiness gates

Before tagging a release:

```bash
cargo test --quiet
cargo build --release
bash -n scripts/install-linux.sh
```

On Windows, also parse the PowerShell installers:

```powershell
$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile(".\scripts\install-windows.ps1", [ref]$null, [ref]$errors) | Out-Null
if ($errors) { throw $errors }
```

The `CI` workflow runs these checks on pull requests and pushes to the default branches. A release tag should not be cut unless CI is green. Use `docs/RELEASE_SIGNOFF.md` for the clean-machine Linux, WSL, Windows, reboot, uninstall, and signing checklist before publishing a paid build. Use `docs/SECURITY.md` for supported network exposure, browser header, data handling, and release integrity rules.

## Backup and restore

Stop the service before taking an offline backup:

```bash
systemctl --user stop palimpsest.service
palimpsest --data-dir ~/.palimpsest --backup-dir ~/palimpsest-backup
systemctl --user start palimpsest.service
```

Restore into the configured data directory:

```bash
palimpsest --data-dir ~/.palimpsest --restore-dir ~/palimpsest-backup --force
```

On Windows, use the same CLI flags with the installed `palimpsest.exe` and stop the Windows service first.

## Remote access

The packaged service installers are local-only. For remote access, run the binary manually or provide a reviewed custom service definition. If you bind to anything other than localhost, set `PALIMPSEST_TOKEN`; otherwise Palimpsest refuses to start.

```bash
PALIMPSEST_TOKEN='replace-with-a-secret' palimpsest --host 0.0.0.0
curl -H "Authorization: Bearer replace-with-a-secret" http://127.0.0.1:3111/health
```
