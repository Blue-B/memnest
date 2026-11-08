# 운영 안내

<!-- markdownlint-disable MD013 -->

[English](operations.md) | [한국어](operations.ko.md)

memnest를 실제 서비스로 돌리는 방법입니다. 필요 환경, 설치, 보존, 복구, 백업, 무언가를 바꾸기 전에 할 확인을 다룹니다.

memnest가 무엇이고 에이전트를 어떻게 연결하는지는 [README](../README.ko.md)부터 보세요.

## 필요 환경

코어 v0.5.0은 Linux x86_64와 Arm64, Apple Silicon macOS용 실행 파일을 제공하며 Git과 Rust가 필요 없습니다. 통합 설치에는 Python 3이 필요합니다. Python 3.10에서는 기존 Codex `config.toml`을 바꾸지 않는데, 그 파일을 검사하려면 Python 3.11이 필요하기 때문입니다. 소스로 빌드하려면 Git과 Rust 2024 edition을 지원하는 Rust 도구가 더 필요합니다. 첫 임베딩 작업 때 설정한 모델을 내려받으려면 인터넷이 필요합니다. Intel Mac은 미리 빌드된 onnxruntime이 없어서 소스로 빌드해야 합니다.

Linux 설치에는 systemd의 `systemctl`과 `systemd-analyze`가 필요합니다. `systemd-analyze`는 업그레이드 전에 사용자 또는 시스템 유닛 검색 경로를 읽기 전용으로 확인하는 데 씁니다. 선택 패키지인 `pi-extension/`의 실행 요구 사항은 그 패키지 README에 있습니다.

## 서비스로 실행

먼저 릴리스 실행 파일을 빌드합니다.

```bash
cd core
cargo build --release
```

### 한 번에 설치

v0.3.0 Linux 릴리스라면, 실행하기 전에 설치 스크립트를 내려받아 검토하세요.

```bash
installer="$(mktemp)"
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/v0.3.0/core/scripts/install.sh \
  -o "$installer"
VERSION=v0.3.0 bash "$installer" --user
```

소스 체크아웃에서는 빌드한 뒤 `core/`에서 실행합니다.

```bash
scripts/setup.sh --user --bin target/release/memnest
```

다운로더는 예전 v0.2.1 압축 파일 구조도 알아보지만, 기존 서비스 위에 그 예전 설치기를 돌리거나 `--autocontext`와 함께 돌리는 것은 거부합니다.

사용자 모드 설치는 서버와 대화 수집기를 설치하고 시작합니다. Claude Code, Codex, Cursor, pi를 찾아서 지원하는 MCP 항목을 합쳐 넣습니다. [작업 카드](task-cards.ko.md) 훅도 추가합니다. `~/.claude/settings.json`과 `~/.codex/hooks.json`의 `SessionStart`와 `UserPromptSubmit`에 `memnest hook --no-recall`을 넣고, Codex가 `task_card`를 승인 질문 없이 부를 수 있게 합니다. Codex는 새 훅을 신뢰한 뒤에만 실행하므로, Codex를 열고 `/hooks`를 한 번 실행하세요. 자동 회상은 기본으로 설치하지 않습니다. 켜려면 `setup.sh`나 `install.sh`에 `--autocontext`를 붙이세요. 이 옵션은 Memnest가 넣은 `UserPromptSubmit` 항목만 회상 형태로 바꿉니다. 이미 있는 Memnest 클라이언트 항목은 바꾸지 않습니다. 바뀌는 클라이언트 파일은 모두 먼저 개인용 시각 폴더에 바이트 그대로 복사합니다. 그 폴더에 목록(manifest)과 자체 복원 스크립트가 있어서, 서비스를 제거해도 되돌리기 도구는 남습니다. 새로 만든 파일에는 `ABSENT` 표시를 남깁니다. 설치는 고유한 임시 기억을 저장하고, 검색하고, 휴지통에 넣는 것으로 끝납니다. 이 첫 왕복 때 임베딩 모델을 내려받을 수 있습니다.

이미 설치했다면 `python3 ~/.local/share/memnest/scripts/setup-clients.py`로 작업 카드 훅을 추가하고, `--autocontext`를 붙이면 회상까지 켭니다. 서비스를 다시 설치할 필요는 없습니다. 기존 프롬프트 훅은 업그레이드 때 조용히 꺼지지 않고 그대로 남습니다. 더는 원하지 않으면 클라이언트의 `UserPromptSubmit` 설정에서 Memnest 명령만 지우세요. pi의 별도 기본 꺼짐 정책은 `MEMNEST_AUTOCONTEXT_MODE`로 정하고 `/memnest`가 보여 줍니다. 명시적 도구와 `memnest watch`는 어느 회상 훅에도 의존하지 않습니다.

설치는 시각 폴더를 쓰는 정확한 복원 명령을 출력합니다. 복원은 먼저 모든 클라이언트 파일의 설치 직후 SHA-256을 확인하고, 그 뒤 사용자나 클라이언트가 바꾼 내용은 덮어쓰지 않습니다. 충돌한 파일을 검토한 뒤에만 `--force-restore`를 쓰세요. 이 옵션은 그 변경을 설치 전 사본으로 일부러 덮어씁니다.

```bash
python3 ~/.memnest/setup-backups/<timestamp>/setup-clients.py \
  --restore ~/.memnest/setup-backups/<timestamp>/manifest.json
```

Linux에서는 systemd를 쓰고 사용자 데이터를 `~/.memnest`에 둡니다. `/var/lib/memnest`를 쓰려면 `--system`을 쓰세요. 시스템 모드는 root로 대화 수집기를 돌리지 않으므로, 수집은 데스크톱 사용자가 실행해야 합니다. macOS는 `~/Library/LaunchAgents`의 `io.memnest.service`와 `io.memnest.watch`를 쓰고, 로그는 `~/Library/Logs/Memnest`에 남습니다. CI는 GitHub의 macOS 14 러너에서 설치, 재설치, 재시작, 제거를 확인하며, 개인 Mac에서는 확인하지 않았습니다.

더 낮은 수준의 Linux 설치기도 그대로 쓸 수 있습니다.

```bash
scripts/preflight-linux.sh --user --bin target/release/memnest
scripts/install-linux.sh --user --bin target/release/memnest
```

### Linux 서비스 업그레이드

소스 설치기는 기존 서버와 수집기 유닛을 템플릿으로 바꾸지 않고 유지합니다. 지원하는 패키지 구조라면 직접 넣은 `Environment=` 항목, 로그, 권한, 다른 설정이 그대로 남습니다. `MEMNEST_HOST`와 `MEMNEST_PORT`를 빼면 설치는 설치된 주소를 읽어서 서비스 확인, 새 클라이언트 항목, 임시 왕복에 씁니다. 기존 클라이언트 항목은 일부러 다시 쓰지 않으니, 주소를 바꾼다면 그 항목은 따로 고치세요.

지원하는 주소 설정을 명시적으로 바꾸려면 값을 설치에 넘깁니다. 예: `MEMNEST_PORT=3222 scripts/setup.sh --user --bin target/release/memnest`. 유닛을 바꾸기 전에 설치기는 그 옆에 개인용 `*.backup.*` 사본을 쓰고 정확한 복원 명령을 출력합니다. 바뀌지 않은 유닛은 다시 쓰지 않습니다. 시스템 모드 준비 단계는 공유 `/tmp/memnest.service` 파일이 아니라 고유한 개인 임시 폴더를 씁니다.

일반적인 systemd 설정 편집기가 아닙니다. 패키지 기본 `ExecStart`와, 따옴표 없는 단독 `Environment=MEMNEST_HOST=...` / `Environment=MEMNEST_PORT=...` 설정이 필요합니다. 직접 만든 명령, 외부 환경 파일, 모호한 주소 설정, 충돌하는 수집기 포트, 심볼릭 링크 유닛, 추가 설정(drop-in)이 있으면 바꾸기 전에 멈춥니다. 패키지 설치는 IPv4 루프백(`127.0.0.1` 또는 `localhost`)만 지원하며, 다른 주소는 직접 설정해야 합니다. 이렇게 거부하는 것은 기존 설정을 조용히 초기화하지 않기 위해서입니다.

지원하지 않는 설정이라면 데이터베이스와 서비스 설정을 백업하고, 해당 서비스를 멈추고, 검토한 `ExecStart` 경로의 실행 파일만 바꾸세요. 유닛과 추가 설정은 그대로 두고, 서비스를 다시 시작한 뒤 실제 주소를 확인합니다. 설치를 진행시키려고 직접 넣은 설정을 지우지 마세요. 의도한 유닛 변경을 되돌리려면 해당 서비스를 멈추고, 바뀐 두 유닛의 출력된 복원 명령을 실행하고, systemd를 다시 읽고, 서비스를 다시 시작합니다. 유닛 백업은 이전 실행 파일과 데이터베이스 백업을 대신하지 않습니다.

`python3 core/scripts/test-install-linux.py`는 일회용 경로에서 사용자/시스템 파일 변화와 설치 주소 일치를 확인합니다. 서비스 관리자, 권한 명령, HTTP 확인은 테스트용 대역이며, 실제 시스템 수준 설치가 아닙니다.

### 되돌리기와 제거

설치 목록이 데이터 폴더 아래에 있으므로, `--remove-data`를 쓰기 전에 클라이언트 설정부터 복원하세요. 제거는 기본으로 기억, 로그, 설정 백업을 남깁니다.

```bash
~/.local/share/memnest/scripts/uninstall-linux.sh --user # Linux
~/.local/share/memnest/scripts/uninstall-macos.sh         # macOS
```

남긴 저장소와 설치 백업을 영구히 지우려 할 때만 `--remove-data`를 넘기세요. 제거는 사용자가 어떤 공유 클라이언트 항목을 계속 원하는지 추측하지 않으니, 먼저 정확한 목록 복원 명령으로 되돌리세요.

설치된 macOS 서비스는 `scripts/validate-installed-macos.sh`로 확인합니다. 코어 CI는 GitHub의 macOS 14 러너에서 설치, 재설치, 재시작 포함 확인, `--remove-data` 제거를 실행합니다. 재부팅 후 로그인 시 자동 시작은 시험하지 않았습니다. 압축 파일은 서명이나 공증이 되어 있지 않습니다. `curl`로 받으면 격리되지 않지만, 브라우저로 받은 파일은 Gatekeeper 승인이 필요할 수 있습니다.

### WSL

저장소 루트에서 PowerShell 설치기를 실행하고, `RepoPath`를 WSL 배포판 안의 `core` 폴더로 지정합니다.

```powershell
.\core\scripts\install-wsl.ps1 -Distro Ubuntu-24.04 -RepoPath /home/<user>/memnest/core
```

이 스크립트는 Linux 사용자 서비스를 설치하고, 로그인 때 그 서비스를 시작하는 Windows 작업을 등록합니다.

WSL 안의 `memnest watch`는 WSL 홈 폴더의 대화 기록을 읽습니다. Windows 쪽에서 실행되는 Claude Code나 Codex의 기록은 `memnest watch --path /mnt/c/Users/<사용자>/.claude/projects`처럼 경로를 지정해야 합니다. 수집기는 5초마다 파일을 확인하는 방식이라 `/mnt/c` 경로에서도 동작할 것으로 보지만, 시험하지는 않았습니다.

### Windows 서비스

`core`에서 `memnest.exe`를 빌드한 뒤, 저장소 루트에서 관리자 PowerShell을 실행합니다.

```powershell
.\core\scripts\preflight-windows.ps1
.\core\scripts\install-windows.ps1 -BinPath .\core\target\release\memnest.exe
```

네이티브 서비스는 데이터를 `%ProgramData%\Memnest\data`에 두고 localhost에 바인드합니다.

서비스 래퍼는 WinSW입니다. 설치기는 한 버전과 그 SHA-256을 고정하고, 래퍼를 내려받았든 스크립트 옆에서 찾았든 설치 전에 파일을 검증하며, 맞지 않으면 파일을 지우고 멈춥니다. 그래서 `-WinSWVersion`을 바꾸려면 맞는 `-WinSWSha256`을 넘기거나 래퍼 옆에 `WinSW-x64.exe.sha256`을 두어야 합니다. 확인할 수 없는 바이트로는 관리자 권한 설치를 하지 않습니다.

Windows 설치기는 `memnest watch`를 자동으로 시작하지 않습니다. 대화 수집이 필요하면 따로 실행해야 하며, Windows 네이티브 수집은 시험하지 않았습니다.

구조별 제거 스크립트는 `core/scripts/`에 있습니다.

## 보존과 복구

보존 기간은 기억 종류와 중요도에 따라 다릅니다.

- 수동 기억과 정리된 기억은 자동으로 만료되지 않습니다.
- knowledge, decision, preference 기억은 자동으로 만료되지 않습니다.
- `.transcript` 출처와 이벤트 식별이 있는 새 대화 AutoLog 기록은 자동으로 만료되지 않습니다.
- 예전 AutoLog 기록은 설정 가능한 기본 30일(`MEMNEST_TTL_AUTOLOG_DAYS`)을 따릅니다.
- 걸러진 기록은 7일 뒤 만료됩니다.
- 고정된 기억은 자동 보존 정리와 일반 정리 요청에서 빠집니다.

만료되거나 직접 삭제한 기억은 먼저 `_trash`로 옮겨지고, 검색에 나오지 않습니다. 그동안은 ID로 복원할 수 있습니다.

```bash
curl -s http://127.0.0.1:3111/restore \
  -H 'content-type: application/json' \
  -d '{"ids":["manual_..."]}'
```

휴지통의 기본 복구 기간은 30일이고, `MEMNEST_TRASH_RETENTION_DAYS`로 0~3650을 정할 수 있습니다. 평문 보관 사본은 기본으로 꺼져 있고, `MEMNEST_ARCHIVE=1`로 직접 켭니다. 이 설정을 바꿔도 기존 보관 파일은 지워지지 않습니다. `POST /purge`는 이미 휴지통에 있는 기록을 운영 저장소와 색인에서 영구히 지울 뿐, 모든 외부 사본이나 물리적 사본을 지우지는 않습니다.

선택 기능인 [팀 접근 정책](team-access.ko.md)은 이름 있는 프로젝트 권한, 접근 감사, 프로젝트별 최대 보관 기간을 더합니다. 이 기간은 설정한 프로젝트에서 고정이나 종류에 따른 예외보다 우선합니다. 관리자는 `POST /retention`으로 수명 주기 확인을 실행할 수 있습니다. 정책과 클라이언트 자격 증명을 준비하지 않았다면, 개인 저장소를 업그레이드할 때는 이 기능을 켜지 마세요.

데이터를 바꾸지 않고 정리를 미리 봅니다.

```bash
curl -s http://127.0.0.1:3111/prune \
  -H 'content-type: application/json' \
  -d '{"project":"root","older_than_days":30,"dry_run":true}'
```

실제 정리와 수명 주기 작업은 `<data-dir>/audit.log`에 기록을 덧붙입니다. `/health`는 최근 수명 주기 실행을 보여 줍니다. `/stats`는 컬렉션 크기, 기간별 분포, 디스크 사용량, 정리 권장 사항을 보여 줍니다.

## 모니터링

`memnest status`는 서버 주소, 대화 수집기의 마지막 신호, 마지막으로 성공한 대화 저장을 보여 줍니다. 신호가 없거나 오래됐으면 기억 서버에 닿더라도 경고로 표시합니다.

`/stats`는 프로세스 메모리에 둔 카운터로 검색 지연을 보여 줍니다. 시작 후 검색 횟수, 평균, 가장 느린 검색입니다. 서비스를 다시 시작하면 초기화됩니다.

검색어는 기록하지 않습니다. 느린 검색은 무엇을 물었는지 사본을 남기지 않고 숫자로만 드러나며, 지난 대화는 `memnest watch`의 대화 기록으로 검색할 수 있습니다.

서비스가 멈출 때 대기 중이거나 실행 중이던 처리 작업은 다음 시작 때 실패로 표시됩니다. 그래서 중단된 작업이 영원히 진행 중처럼 보이지 않습니다.

## 작업공간 범위와 색인 복구

클라이언트가 `cwd`를 주면 memnest는 정규화한 절대 경로에서 되돌릴 수 없는 공개 작업공간 ID를 만듭니다. 그 작업공간은 자기 기록과 `playbook`을 검색합니다. 범위를 넘어 찾는 방법은 여전히 `project=all`뿐입니다. 예전 폴더 이름 컬렉션은 그 이름이 등록된 작업공간 하나에만 속할 때만 포함되며, 모호한 별칭은 추측하지 않고 꺼 둡니다.

SQLite가 기준입니다. 모든 조각의 추가, 수정, 대체, 휴지통 이동, 복원, 영구 삭제는 같은 트랜잭션에서 `index_queue` 행을 씁니다. Tantivy와 HNSW 변경은 두 색인이 모두 안전하게 기록된 뒤에만 그 행을 지웁니다. 남은 작업, 없는 파일, 예전 색인 구조, 손상된 HNSW 보조 파일이 있으면 행 수 제한 없이 모든 SQLite 행에서 색인을 처음부터 다시 만듭니다. 색인 폴더는 실제 폴더 옆에서 준비한 뒤 이름을 바꿔 교체합니다. 한 데이터 폴더에는 한 프로세스만 쓸 수 있습니다.

## 백업과 복원

백업은 서비스가 돌아가는 동안에도 할 수 있습니다. CLI는 SQLite `VACUUM INTO`로 일관된 데이터베이스 스냅샷을 만들고, 지속되는 보조 파일을 복사하고, 다시 만들 수 있는 색인과 모델 캐시는 빼고, SQLite와 암호화된 금고 행을 검증한 뒤 준비한 폴더의 이름을 바꿔 자리에 놓습니다.

```bash
memnest --data-dir ~/.memnest --backup-dir ~/memnest-backup
```

복원 전에는 서비스를 멈추세요. 복원은 원본과 대상 경로가 겹치면 거부합니다. 먼저 옆에 임시 사본을 만들어 검증한 뒤 폴더를 바꿉니다. `--force`는 비어 있지 않은 대상을 바꾸는 것을 허용하지만, 준비한 사본이 검증을 통과하기 전에는 대상을 지우지 않습니다.

```bash
memnest --data-dir ~/.memnest --restore-dir ~/memnest-backup --force
```

백업에는 `master.key`가 들어가지만, 이 키의 보호된 사본을 하나 더 보관하세요.

새 금고 행은 `$enc2$` 암호문을 쓰며, AES-GCM 연관 데이터에 비밀 키 이름이나 서버 이름이 들어갑니다. 그래서 암호문을 다른 행으로 옮기면 복호화가 실패합니다. 기존 `$enc$` 행은 호환 복호화 경로를 쓰고, 다시 쓰기 전까지 읽을 수 있습니다. 모델이 쓰는 비밀 도구는 `MEMNEST_EXPOSE_SECRET_TOOLS=1`이 아니면 숨겨지며, localhost HTTP 금고 API는 그대로 쓸 수 있습니다.

## CLI 참고

```bash
memnest status                                   # 서비스 상태와 주소
memnest --data-dir ~/.memnest                    # HTTP API와 POST /mcp의 MCP
memnest --mcp --data-dir ~/.memnest              # stdio MCP 서버
memnest hook                                     # 호스트 프롬프트 훅에 작업 카드와 맥락 묶음으로 응답
memnest hook --no-recall                         # 작업 카드만
memnest watch                                    # 세션 대화 기록을 따라가며 새 대화 저장
memnest --doctor --data-dir ~/.memnest           # 환경과 저장소 점검
memnest --warmup-embedding --data-dir ~/.memnest # 처음 쓰기 전에 모델 미리 받기
memnest --help
```

자주 쓰는 옵션: `--host`, `--port`, `--data-dir`, `--backup-dir`, `--restore-dir`, `--import-jsonl`.

`hook`은 호스트의 훅 내용을 표준 입력으로 읽고 답을 표준 출력으로 씁니다. `--format`으로 고정하지 않으면 입력 내용을 보고 형태를 고릅니다. 프롬프트를 막지 않습니다. 서비스에 닿지 않으면 아무것도 출력하지 않고 0으로 끝납니다. `watch`는 Claude Code, pi, Codex 대화 기록의 유일한 자동 수집 경로입니다. 파일마다 바이트 위치를 `<data-dir>/watch-state.json`에 두고, `--backfill`로 기존 기록을 요청하지 않으면 새 파일은 끝에서부터 따라가며, 저장이 성공하거나 중복 없는 재시도가 확인된 뒤에만 위치를 옮깁니다. 둘 다 HTTP로 서비스와 통신하며 `--url`을 받고, 없으면 `MEMNEST_URL`을 씁니다.

이미 추적한 과거 기록은 `memnest watch --path /absolute/original/session.jsonl --backfill --once --dry-run`으로 미리 보고, `--dry-run`을 빼서 가져오세요. 이 명시적 재생은 실행 중인 수집 위치를 무시하되 바꾸지 않고, 정해진 크기로 나눠 전체 기록을 처리하며, 새 조각과 건너뛴 조각을 따로 보고합니다. 가져오기에 실패하면 0이 아닌 코드로 끝나고, 같은 원본으로 다시 시도하면 기존 중복 제거와 삭제 표시가 적용됩니다. 미리 보기는 대상 텍스트를 셀 뿐, DB에 빠진 것을 세지 않습니다. 검증 계획과 식별 한계는 [과거 기록 가져오기](history-import.ko.md)를 보세요.

`--viewer-port`는 더 이상 쓰지 않습니다. 모든 주소는 `--port`에서 제공합니다.

## 설정

| 변수 | 효과 |
| --- | --- |
| `MEMNEST_DATA_DIR` | 데이터 폴더. `--data-dir`과 같음 |
| `MEMNEST_TOKEN` | 로컬이 아닌 주소에 바인드할 때 필수. 클라이언트는 `Authorization: Bearer <token>`을 보냄 |
| `MEMNEST_EMBED_MODEL` | 임베딩 모델. 기본값 `intfloat/multilingual-e5-base` |
| `MEMNEST_EMBED_DIM` | 임베딩 차원. 기본값 768 |
| `MEMNEST_TTL_AUTOLOG_DAYS` | 예전 AutoLog 보존 기간. 기본값 30. 대화 이벤트는 영구 보존 |
| `MEMNEST_ARCHIVE` | `0`이면 영구 삭제 전에 보관 JSONL을 쓰지 않음 |
| `MEMNEST_EXPOSE_SECRET_TOOLS` | `1`이면 MCP와 pi 확장에 금고 도구 4개를 공개. 기본은 숨김 |
| `MEMNEST_REBUILD_INDEXES` | `1`이면 시작할 때 SQLite에서 Tantivy/HNSW를 모두 다시 만듦 |

로컬 클라이언트가 아닌 곳에서 HTTP API에 닿으려면 토큰만으로는 부족합니다. ChatGPT, Claude 커스텀 커넥터, Grok 같은 클라우드 AI는 자기 서버에서 서비스를 부르므로 공개 주소가 필요하고, 모든 클라이언트가 bearer 헤더를 보낼 수 있지도 않습니다. [클라우드 AI 연결](web-connectors.ko.md)을 보세요.

## 개발 검사

명령 하나로 모두 돌립니다. 빌드하고, 각 패키지 테스트를 자기 포트의 임시 인스턴스에 대고 돌리고, 문서 약속을 확인합니다.

```bash
scripts/preflight.sh
```

개별 명령보다 이것을 권합니다. 모든 테스트를 임시 인스턴스 하나에 대고 돌리므로, 각 테스트가 따로는 통과하더라도 `core/` 변경이 의존 패키지를 깨는 것을 잡습니다.

하나씩 돌리고 싶다면 아래 명령을 씁니다. 각 블록은 하위 셸이라 매 줄이 저장소 루트에서 시작합니다.

```bash
(cd core                  && cargo check && cargo test --locked -- --test-threads=1)
(cd pi-extension          && npm install && npm run build && npm run smoke)
(cd adapters/generic-http && node test.mjs)
```

실행 중인 서비스에 대고 `scripts/verify-contract.sh`를 돌리면, 코드가 아니라 이 문서의 주장을 확인합니다. 문서가 알리는 모든 주소를 부르고, README의 도구 목록을 `tools/list` 결과와 비교하고, 없앤 주소가 404를 주는지, 여기 적힌 파일과 CLI 하위 명령이 있는지 확인합니다. 내 저장소가 아니라 임시 인스턴스에 대고 돌리세요.

```bash
scripts/verify-contract.sh http://127.0.0.1:3150 ./target/release/memnest /tmp/memnest-scratch
```

코어 테스트는 순서대로 돌립니다. 환경 변수와 금고 수명 주기 테스트가 프로세스 전역 상태를 공유해서, 기본 병렬 Rust 테스트 실행에서는 서로 간섭하기 때문입니다. CI도 같은 옵션을 씁니다.

스모크 테스트는 받은 주소에 응답하는 저장소에 씁니다. 실제로 쓰는 저장소를 가리키지 마세요. [CONTRIBUTING.ko.md](../CONTRIBUTING.ko.md)를 보세요.

## 읽기 전용 첫 사용 진단

주소를 명시적으로 지정합니다.

```bash
core/target/debug/memnest --host 127.0.0.1 --port 3111 status --diagnose
# 인증을 설정했다면 기존 MEMNEST_TOKEN을 환경 변수로 비공개로 넘기세요.
# 로그, URL, 셸 기록에 붙여 넣지 마세요.
```

이 명령은 `GET /health`와 MCP `tools/list`(POST `/mcp`)만 실행합니다. HTTP 전체 시간은 5초, 응답 하나는 256 KiB로 제한하고, 리다이렉트와 프록시를 쓰지 않으며, DB, 색인, 모델을 초기화하지 않습니다. 종료 코드 0은 상태 약속과, 범위 검색/페이지 조회 필드가 알려져 있다는 뜻이며, 임베딩 성공이나 완전한 클라이언트 호환을 뜻하지 **않습니다**. 종료 코드 1은 연결 불가, HTTP 401/403 인증 실패, 잘못되거나 오래된 기능 약속, 너무 큰 응답, 시간 초과를 구분해 알려 줍니다. 기억 텍스트를 보내거나, 기억 도구를 부르거나, 클라이언트 설정을 바꾸거나, 토큰을 출력하지 않습니다. 믿을 수 있는 주소에만 쓰세요. 루프백이 아닌 HTTP 주소를 지정하면 bearer 토큰이 TLS 없이 전송됩니다. 이 옵션은 `MEMNEST_URL`이 아니라 `--host`/`--port`를 씁니다.

v0.3.0 상태 약속은 임베딩 모델이 지금 올라와 있는지, 대기 중인 색인 작업 수, 재구축이 필요한지를 알려 줍니다. 모델이 안 올라와 있는 것은 보통 실패가 아니라 지연 로딩이며, 첫 저장이나 검색 때 초기화됩니다. 예전 상태 약속은 알 수 없음으로 표시합니다. 서비스 상태에는 수집기 상태가 없으므로 원격 수집은 알 수 없음으로 남습니다. `memnest --data-dir <watcher-state-dir> status`는 로컬 신호와 마지막 저장 시각을 읽으며, 이 폴더는 서버 데이터 폴더와 다를 수 있습니다. 신호가 최신이라고 모든 대화 기록이 색인됐다는 증거는 아닙니다. 진단 모드는 수집기 상태 파일을 읽지 않습니다. 기존 `--doctor`는 이 읽기 전용 경로가 **아닙니다**. 시험 파일을 쓰고 색인을 열므로, 실행 중인 서비스를 바꾸지 않는 확인용으로 쓰지 마세요.

복구: 연결 불가 → 의도한 주소와 서비스 로그를 확인합니다. 인증 실패 → 서버와 클라이언트의 토큰 설정을 출력하지 않고 비교합니다. 기능 불일치 → 맞는 소스 코어/클라이언트 빌드나 출시된 기능을 씁니다. 토큰이나 구조 불일치를 고치려고 DB를 지우거나 클라이언트 설정을 다시 설치하지 마세요. 데이터 복구는 이전 실행 파일과 데이터 폴더 전체를 남긴 채 위의 오프라인 백업/복원 절차를 쓰세요. 클라이언트 설정 백업은 기억 백업이 아닙니다.

### 일회용 첫 기억/검색 확인

실제 대화를 설치하거나 가져오는 것이 아니라 **임시 데모**용입니다.

```bash
cd core && cargo build --offline && cd ..
python3 scripts/test-status-diagnostic.py
python3 scripts/evaluate-coding-memory.py --output /tmp/memnest-evaluation.json
```

평가에는 이미 채워진 `core/target/test-model-cache`가 필요합니다. 캐시가 없으면 거부하고 모델 다운로드를 막습니다. 현재 고정 자료는 기록 16개와 직접 쓴 질문 48개이며, 그중 26개는 묻는 사실이 없는 질문입니다. 새 임시 DB, 임의 루프백 주소, 일회용 토큰을 만들고, 쓰기 전에 서비스 정체를 확인하고, 고정 자료를 저장하고 검색한 뒤, 자기 데이터와 자식 프로세스만 지웁니다. JSON 결과와 옆의 `.daemon.log`는 출력 경로에 남습니다. 실제 설치는 건드리지 않습니다. 실제로 보관할 사용자 데이터는 설정한 서비스 데이터 폴더에 있어야 하며, 이 임시 데모에 두면 안 됩니다. 설치의 별도 임시 왕복은 설정한 서비스에 *실제로* 쓴 뒤 그 확인용 기억을 휴지통에 넣습니다. 자동 회상은 직접 켜야 하며, 평가는 수집이나 회상을 설치하지 않습니다.

`scripts/preflight.sh`는 예전 릴리스 확인용이며 이 안전한 데모가 아닙니다. 고정/공유 기본값을 쓰고 모델을 내려받을 수 있습니다. 다운로드 없는 격리 검증에는 쓰지 마세요. 고정 자료와 측정된 한계는 [제품 계획](product-upgrade-plan.ko.md)을 보세요.

진단 종료 코드는 기계가 읽을 수 있습니다. 두 약속이 맞으면 **0**, 진단 실패면 **1**입니다. 실패 종류는 지금은 숫자 코드나 JSON이 아니라 사람이 읽는 고정 문구로 나옵니다. HTTP 응답 문자열(`version`, 오류, 도구 설명 포함)은 그대로 출력하지 않습니다. 자격 증명, 서버 로컬 경로, 터미널 제어 문자가 들어 있을 수 있기 때문입니다. 명시적으로 요청한 주소는 출력하지만, 로컬 `--data-dir`과 원격 상태의 `data_dir`은 출력하지 않습니다. CLI 버전과 로컬 수집 보고는 일반 `status`를 쓰세요. 저장된 평가 JSON은 측정한 증거를 다시 쓰지 않으려고, 보안 강화 전의 원래 진단 기록을 그대로 둡니다.

검색 점수는 여러 요소를 합친 순위 점수이며 **관련성 확률이 아닙니다**. 고정 자료의 "정답 없음 → 빈 결과" 비율은 직접 검색을 잰 것이고, 자동 맥락 주입이 참는지를 잰 것이 아닙니다. 선택 기능인 pi 회상에는 범위, 종류, 점수 관문이 따로 있고 기본으로 꺼져 있으며, 이 평가는 그 관문이 얼마나 자주 주입하거나 참는지 증명하지 않습니다.
