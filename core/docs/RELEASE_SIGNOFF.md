# Release signoff

Use this checklist before publishing a paid Palimpsest build.

## Build gate

Run from a clean checkout or in CI:

```bash
cargo test --quiet
cargo build --release
scripts/verify-release.sh
```

Verify downloaded release archives before installing:

```bash
scripts/verify-artifact.sh palimpsest-v1.0.0-x86_64-unknown-linux-gnu.tar.gz palimpsest-v1.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

On Windows:

```powershell
.\scripts\verify-release.ps1
cargo test --quiet
cargo build --release
.\scripts\verify-artifact-windows.ps1 -Artifact .\palimpsest-v1.0.0-x86_64-pc-windows-msvc.zip -Checksum .\palimpsest-v1.0.0-x86_64-pc-windows-msvc.zip.sha256
```

## Clean Linux VM

1. Download and extract the Linux release archive.
2. Install and validate:

```bash
scripts/install-linux.sh --user --bin ./palimpsest
scripts/validate-installed.sh --user
~/.local/bin/palimpsest --data-dir ~/.palimpsest --warmup-embedding
```

3. Reboot the VM and validate again:

```bash
scripts/validate-installed.sh --user
```

4. Uninstall:

```bash
scripts/uninstall-linux.sh --user
```

## Clean WSL Distro

1. Download and extract the Linux release archive inside the distro.
2. Install and validate inside WSL:

```bash
scripts/install-linux.sh --user --bin ./palimpsest
scripts/validate-installed.sh --user
```

3. From Windows PowerShell, register the wake task:

```powershell
.\scripts\install-wsl.ps1 -Distro Ubuntu-24.04 -RegisterTaskOnly
```

4. Run `wsl --shutdown`, start the scheduled task or open the distro, then validate:

```powershell
Start-ScheduledTask -TaskName "Palimpsest WSL"
wsl -d Ubuntu-24.04 -- bash -lc "cd /path/to/palimpsest && scripts/validate-installed.sh --user"
wsl -d Ubuntu-24.04 -- bash -lc "$HOME/.local/bin/palimpsest --data-dir $HOME/.palimpsest --warmup-embedding"
```

## Clean Windows VM

1. Download and extract the Windows release archive.
2. Run elevated PowerShell:

```powershell
.\scripts\install-windows.ps1
.\scripts\validate-installed-windows.ps1
& "$env:ProgramData\Palimpsest\app\palimpsest.exe" --data-dir "$env:ProgramData\Palimpsest\data" --warmup-embedding
Get-CimInstance Win32_Service -Filter "Name='palimpsest'" | Select-Object Name, StartName, State, PathName
```

3. Reboot the VM and validate again:

```powershell
.\scripts\validate-installed-windows.ps1
```

4. Uninstall:

```powershell
.\scripts\uninstall-windows.ps1
```

## Signing Gate

- Windows binaries are signed.
- Windows PowerShell installer, validator, support, and signing scripts are signed.
- Release checksums are published with the release.
- Release notes identify supported targets: Linux native, WSL, and Windows native.
- GitHub Actions secrets `WINDOWS_CODESIGN_PFX_BASE64` and `WINDOWS_CODESIGN_PASSWORD` are configured for automatic Windows binary signing.

Example signing command:

```powershell
$files = @(".\palimpsest.exe", ".\WinSW-x64.exe") + (Get-ChildItem .\scripts -Filter *.ps1 -File | Select-Object -ExpandProperty FullName)
.\scripts\sign-windows.ps1 -FilePath $files -CertificatePath C:\secure\codesign.pfx -CertificatePassword "<password>"
.\scripts\verify-windows-signatures.ps1 -FilePath $files
```
