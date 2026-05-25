# Security

Palimpsest is a local-first memory service. The supported default deployment binds to `127.0.0.1:3111` and stores data on the user's machine.

## Network exposure

- The service refuses non-localhost binds unless `PALIMPSEST_TOKEN` is set.
- When `PALIMPSEST_TOKEN` is set, API calls must include `Authorization: Bearer <token>`.
- Do not expose Palimpsest directly to the public internet. Put a reviewed reverse proxy and TLS policy in front of it if remote access is required.

## Dashboard headers

HTTP responses include conservative browser headers:

- `Content-Security-Policy`
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: no-referrer`
- `Permissions-Policy`
- `Cross-Origin-Resource-Policy: same-origin`
- `Cache-Control: no-store`

## Data handling

- Linux user service data: `~/.palimpsest`
- Linux system service data: `/var/lib/palimpsest`
- Windows native service data: `%ProgramData%\Palimpsest\data`
- Manual runs should pass `--data-dir` or set `PALIMPSEST_DATA_DIR`.

Backups are offline copies. Stop the service before taking or restoring a backup.

## Service identity

- Linux user installs run under the installing user's systemd user manager.
- Linux system installs run under the system service manager with data isolated in `/var/lib/palimpsest`.
- Windows native installs use WinSW and require elevated PowerShell for service registration. Before a paid Windows release is approved, verify the installed service identity on a clean Windows VM and ensure the account has access only to the Palimpsest application, data, and log directories needed for operation.
- Do not run a remote-access configuration with broad service privileges. Use the packaged local-only installers unless a reviewed deployment policy says otherwise.

## Release integrity

- Release archives are published with SHA-256 checksum files.
- Windows release archives include `WinSW-x64.exe` and `WinSW-x64.exe.sha256`.
- Paid Windows builds must be signed with the production code-signing certificate before distribution.
