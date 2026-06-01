# Product readiness

Memnest is not considered sellable until every required gate below has evidence.

## Completed in repo

- Local dashboard served on `127.0.0.1:3111`.
- Linux systemd user service template.
- Linux systemd system service template.
- WSL installer that installs inside WSL and registers a Windows logon task.
- Windows native installer that wraps `memnest.exe` with WinSW.
- Windows release archives include the WinSW wrapper and checksum; the installer also supports a pre-approved wrapper override.
- Linux, WSL, and Windows uninstall scripts.
- Linux and Windows preflight scripts catch install blockers before changing service state.
- Installed-service validation scripts for Linux and Windows include health, doctor diagnostics, and restart recovery.
- Support bundle scripts collect non-secret Linux and Windows diagnostics for customer support.
- Release artifacts include `scripts/`, `packaging/`, and `docs/`.
- Release artifacts and installers include dashboard static assets under `static/`.
- CI validates Linux Rust tests/build, Windows Rust tests/build, and installer script syntax.
- Local smoke test covers health, dashboard load, low startup RSS before search, process restart, backup/restore, and remote-bind refusal.
- Search results highlight visible query matches and show excerpts around the match.
- SQLite legacy schema migration is covered by an automated open-time migration test.
- Vector and text indexes are covered by current-version save/reopen search tests, and startup avoids duplicating persisted vector entries.
- Incompatible text-index schemas are treated as rebuildable derived data and recreated automatically.
- Text index readers are loaded lazily so dashboard startup does not immediately pay search-index memory cost.
- Service shutdown handles SIGTERM on Unix and saves the vector index before exit.
- CLI supports offline backup and restore through `--backup-dir` and `--restore-dir`.
- Non-localhost server binds are refused unless `MEMNEST_TOKEN` is configured.
- Dashboard/API responses include baseline browser security headers, verified in smoke tests.
- Runtime data directory can be set with `MEMNEST_DATA_DIR` or `--data-dir`.
- `memnest --warmup-embedding` verifies and warms the local embedding model cache before offline use.
- `memnest --doctor` warns when the embedding model cache has not been warmed yet.
- Update channel is documented as installer-managed upgrade.
- Release signoff checklist defines clean-machine Linux, WSL, Windows, reboot, uninstall, and signing gates.
- Product audit maps requirements to concrete repository evidence and lists external gates.
- Release workflow publishes SHA-256 checksum files with release archives.
- Release archive checksum verification scripts are included for Linux and Windows users.
- Linux release installer verifies downloaded archive checksums before extraction.
- Workflow integrity checks guard required CI/release properties such as signing, checksums, and packaged support files.
- Product audit checks guard dashboard search behavior, i18n hooks, offline dashboard assets, installer coverage, license screening, and external paid-release gate documentation.
- Windows release workflow requires code-signing secrets, then signs and verifies binaries plus PowerShell scripts before packaging.
- Security documentation covers network exposure, response headers, data paths, and release integrity.
- Third-party license metadata check and notices are included in release gates.
- Support docs cover install, service status, logs, backup, restore, WSL wakeup, and remote bind failures.

## Required before paid distribution

- Run installer end-to-end on clean Linux VM.
- Run embedding warmup on the clean Linux VM and confirm `doctor` reports the cache present.
- Run installer end-to-end on clean WSL distro.
- Run embedding warmup inside the clean WSL distro and confirm `doctor` reports the cache present.
- Run installer end-to-end on clean Windows VM with elevated PowerShell.
- Run embedding warmup on the clean Windows VM and confirm `doctor` reports the cache present.
- Run Windows signing with the production certificate and verify signatures on a clean Windows VM.
- Add frozen compatibility fixtures for vector index and text index after the first signed release.
- Add clean-machine sleep, reboot, and service-manager failure recovery tests.

## Supported desktop targets

- Linux native uses systemd. If the distribution does not run systemd, the binary can run manually but the packaged service installer is not a supported path.
- WSL uses the Linux service inside the distro and a Windows Scheduled Task only to wake the distro at Windows logon. If Windows sleeps or shuts WSL down, the service restarts when the task or any WSL session wakes the distro.
- Windows native uses the Windows service manager through WinSW, stores service data under `%ProgramData%\Memnest`, and does not require WSL.
- macOS is not a paid supported target until a launchd installer, signing, notarization, and clean-machine validation are added.

## Release gate commands

```bash
cargo test --quiet
cargo build --release
scripts/verify-release.sh
```

```powershell
$files = @(
  ".\scripts\install-windows.ps1",
  ".\scripts\install-wsl.ps1",
  ".\scripts\uninstall-windows.ps1",
  ".\scripts\uninstall-wsl.ps1"
)
foreach ($file in $files) {
  $tokens = $null
  $errors = $null
  [System.Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$errors) | Out-Null
  if ($errors.Count -gt 0) { throw $errors }
}
.\scripts\verify-release.ps1
```

## Support paths

- Dashboard: `http://127.0.0.1:3111/`
- Health: `http://127.0.0.1:3111/health`
- Linux user logs: `journalctl --user -u memnest.service`
- Linux system logs: `journalctl -u memnest.service`
- Windows logs: `%ProgramData%\Memnest\logs`
