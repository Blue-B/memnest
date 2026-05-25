# Product audit

This audit maps the productization request to concrete repository evidence.

## Objective

Make Palimpsest credible as a paid local product across Linux native, WSL, and Windows native installs, with a dashboard, search behavior, service lifecycle, release packaging, supportability, and security posture that can be validated before sale.

## Evidence-backed deliverables

| Requirement | Evidence |
| --- | --- |
| Search results should not show unrelated items under the best match | Viewer search uses visible-match filtering and excerpts; `scripts/verify-release.sh` runs smoke gates. |
| Search terms should be highlighted | Dashboard search renders query matches with `<mark>` and query excerpts in `src/server/api.rs`. |
| Dashboard search should not load the heavy embedding model for exact visible-match searches | Viewer search skips vector query encoding when visible-match filtering is required; current service stayed around 21 MB RSS before and after a dashboard search. |
| Collections/project basis should be clear | Product docs explain runtime data dirs and project metadata behavior in deployment/readiness docs. |
| Dashboard should be productized, not raw JSON | Dashboard routes are in `src/server/api.rs`; raw API endpoints remain separate from viewer routes. |
| Dashboard should not depend on external CDNs that conflict with CSP or offline installs | Viewer HTML uses local CSS and local assets only; `scripts/smoke-local.sh` fails if Tailwind CDN or Google Fonts references return. |
| Dashboard language switching should cover dynamic product text | Dynamic collection option counts, memory counts, search result counts, and empty result text expose data attributes handled by the local i18n runtime. |
| Linux native install | `scripts/install-linux.sh`, `packaging/systemd/palimpsest.service`, `packaging/systemd/palimpsest-user.service`. |
| WSL install and wake behavior | `scripts/install-wsl.ps1` plus Linux user service; `docs/DEPLOYMENT.md` documents WSL wake-task behavior. |
| Windows native install without WSL | `scripts/install-windows.ps1`, `packaging/windows/palimpsest-service.xml`, WinSW wrapper handling. |
| Windows service administrator requirement | Windows preflight, install, validate, and uninstall scripts require elevated PowerShell before service operations. |
| Windows service data location | Windows native defaults to `%ProgramData%\Palimpsest` so service data is not tied to an interactive user's profile. |
| Windows service identity review | `docs/SECURITY.md` and `docs/RELEASE_SIGNOFF.md` require clean-VM service identity and privilege review before paid Windows release approval. |
| Windows custom port install | `scripts/install-windows.ps1` applies `-Port` to both service arguments and health checks. |
| Uninstall paths | `scripts/uninstall-linux.sh`, `scripts/uninstall-windows.ps1`, `scripts/uninstall-wsl.ps1`. |
| Uninstall should remove app assets without deleting memory data by default | Linux/WSL uninstallers remove installed binaries and static assets; data deletion remains opt-in via `--remove-data` / `-RemoveData`. |
| Install blocker detection | `scripts/preflight-linux.sh`, `scripts/preflight-windows.ps1`. |
| Installed service validation | `scripts/validate-installed.sh`, `scripts/validate-installed-windows.ps1`; both run health checks, doctor diagnostics, and restart recovery. |
| Backup and restore | CLI flags `--backup-dir`, `--restore-dir`, `--force`; verified by `scripts/smoke-local.sh`. |
| Remote bind safety | Non-localhost binds require `PALIMPSEST_TOKEN`; verified by `scripts/smoke-local.sh`. |
| Packaged service network policy | Linux and Windows installers/preflight reject non-local binds so packaged installs remain local-only. |
| Browser security headers | Middleware in `src/server/mod.rs`; header checks in `scripts/smoke-local.sh`. |
| Startup memory should stay low before search | Text index lazy load in `src/lib.rs`; low startup RSS gate in `scripts/smoke-local.sh`. |
| First-use embedding model readiness should be visible before offline use | `palimpsest --warmup-embedding` warms the model cache; `palimpsest --doctor` warns when the cache is not warmed. |
| Index restart stability | Vector index persistence tests, text index reopen tests, no vector duplication on restart test. |
| Incompatible derived text index should recover | Text index schema compatibility check and recreate path in `src/index/mod.rs`. |
| SIGTERM/service shutdown should save index | Unix SIGTERM handler in `src/main.rs`. |
| Release archives include docs/scripts/packaging | Manifest checks in `scripts/verify-release.sh`. |
| Dashboard static assets survive packaged installs | Linux and Windows installers copy `static/`; installed validators fetch `/assets/memory-atlas.png`; release manifest checks include `static/memory-atlas.png`. |
| Release workflow integrity | `scripts/verify-workflows.sh` checks required CI/release workflow properties such as Windows signing, checksums, WinSW inclusion, and release contents. |
| Product requirements remain tied to artifacts | `scripts/product-audit.sh` checks dashboard search behavior, i18n hooks, offline asset policy, installer static asset handling, license gate presence, release signing requirements, and external gate documentation. |
| Windows release includes WinSW and checksum | Release workflow downloads `WinSW-x64.exe`, writes `WinSW-x64.exe.sha256`, and packages both. |
| Release archive checksums | Release workflow writes `.sha256` files for release archives. |
| Release archive verification | `scripts/verify-artifact.sh` and `scripts/verify-artifact-windows.ps1` verify downloaded archives before install. |
| Linux release installer integrity | `scripts/install.sh` downloads the `.sha256` file and refuses checksum mismatches before extraction. |
| Windows code signing path | `scripts/sign-windows.ps1`, `scripts/verify-windows-signatures.ps1`, required Windows binary and PowerShell script signing in `.github/workflows/release.yml`. |
| Customer support diagnostics | `scripts/support-bundle.sh`, `scripts/support-bundle-windows.ps1`. |
| Security documentation | `docs/SECURITY.md`. |
| Third-party license gate | `scripts/check-licenses.py` and `docs/THIRD_PARTY_NOTICES.md` are included in release verification. |
| Release signoff checklist | `docs/RELEASE_SIGNOFF.md`. |

## Latest local verification

- `scripts/verify-release.sh`: passed.
- `target/release/palimpsest --help`: exposes `--warmup-embedding`.
- `target/release/palimpsest --doctor` against an empty temp data directory: warned that the model cache is not warmed.
- Dashboard search for `안녕` on the current service: 5 result cards, 6 highlighted marks, RSS stayed near 21 MB before and after the request.
- `scripts/preflight-linux.sh --user --bin target/release/palimpsest`: passed on the current WSL environment.
- PowerShell parser validation for Windows scripts: passed.
- `scripts/validate-installed.sh --user`: passed on the current WSL service.
- Installed dashboard static asset check: `/assets/memory-atlas.png` returned 2,469,691 bytes on the current WSL service.
- Installed dashboard dependency check: no `cdn.tailwindcss.com` or `fonts.googleapis.com` references; CSP remains self-hosted.
- Current service health: `{"status":"ok","version":"0.1.0"}`.
- Current service state: enabled and active.
- Startup RSS after lazy text-index loading: observed around 4-5 MB before search on the current WSL service.

## Not complete until externally verified

These gates require external machines, credentials, or release infrastructure and cannot be honestly marked complete from this workspace alone.

- Clean Linux VM: install, validate, reboot, validate again, uninstall.
- Clean WSL distro: install, validate, `wsl --shutdown`, wake, validate again, uninstall.
- Clean Windows VM: elevated install, validate, reboot, validate again, uninstall.
- Production Windows code-signing certificate: configure GitHub secrets, build a tagged release, verify signed binaries on a clean Windows VM.
- First signed release compatibility fixtures: freeze vector/text index fixtures from the signed release and keep future compatibility tests against them.
- Sleep/resume testing on real hardware or VM snapshots.
