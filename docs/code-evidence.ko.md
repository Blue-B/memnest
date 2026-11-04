# 선택한 파일 근거 확인

[English](code-evidence.md) | [한국어](code-evidence.ko.md)

코어 0.4.0은 호출자가 보고한 해결 방법을, 직접 고른 작업 파일의 지문과 함께 저장합니다. 나중에 명시적으로 확인을 요청하면 그 파일을 저장된 지문과 비교합니다. 수정할 때마다 경고하거나, 검색 순위를 바꾸거나, 해결 방법을 검증하거나, 기억된 명령을 실행하지는 않습니다.

코어와 CLI는 둘 다 0.4.0을 맞춰 쓰세요. CLI는 더 오래된 코어에는 쓰기 전에 저장을 거부합니다. 기본 읽기는 호환되지만, 새 옵션은 업데이트된 코어가 필요합니다.

## 저장과 확인

코어를 빌드한 뒤(`cd core && cargo build --offline`), 같은 소스로 돌아가는 개발용 서비스에 그 실행 파일을 씁니다. `--url`은 로컬 루프백 HTTP 서비스여야 하고, 기본값은 `MEMNEST_URL`, 그다음 `http://127.0.0.1:3111`입니다. 인증을 켰다면 `MEMNEST_TOKEN`을 인자가 아니라 환경 변수로 넘깁니다.

보고와 연결할 파일이 있는 작업공간에서 실행합니다.

```sh
/path/to/new/memnest evidence --cwd "$PWD" --url "$MEMNEST_URL" remember \
  --text "Build repair: the compiler change failed in this environment" \
  --file Cargo.toml \
  --file src/main.rs \
  --status failed \
  --applicability "Describe the OS, dependency versions, and exact attempted change" \
  --evidence "Describe the observed error or cite the existing test report"
```

경로는 `--cwd` 기준 상대 경로입니다. 해결 방법과 관련 있는 파일만 고르세요. 이 명령은 파일의 지금 내용을 기록하며, 과거 작업 당시의 상태가 아닙니다. 옛 대화에서 옛 기준을 되살릴 수 없습니다. 오늘의 파일을 과거 결과의 독립 증거처럼 붙이지 마세요.

상태는 필수이며 `proposed`, `failed`, `reported_success` 중 하나입니다. 적용 조건과 결과 설명은 계속 호출자의 보고로 취급합니다. [해결 기록](approach-outcomes.md)(영문)을 참고하세요.

응답은 일반 저장 결과와 같고 기억 ID가 들어 있습니다. 나중에 확인할 때 그 ID를 넘깁니다.

```sh
/path/to/new/memnest evidence --cwd "$PWD" --url "$MEMNEST_URL" check "$MEMORY_ID"
```

확인 명령은 JSON을 출력하고 기억을 바꾸지 않습니다. 기준과 제한된 메타데이터만 가져오고 대화 전체는 가져오지 않습니다. 파일이 바뀌었거나 비교할 수 없어도 확인 자체가 성공하면 종료 코드는 0이니, JSON의 상태를 보세요. 통신, 인증, 잘못된 메타데이터, 명령 오류는 0이 아닌 코드로 끝납니다.

| 상태 | 의미 |
| --- | --- |
| `unchanged` | 고른 파일의 내용이 모두 같습니다. 해결 방법이나 적용 조건을 검증한 것은 아닙니다. |
| `changed` | 고른 파일 중 하나 이상이 다르거나 없습니다. 해결 방법이 무효라는 뜻은 아닙니다. |
| `unavailable` | 고른 파일 중 하나 이상을 안전하게 읽지 못했습니다. 예: 심볼릭 링크, 너무 큰 파일. 다른 파일 결과는 있을 수 있습니다. |
| `different_workspace` | 로컬 작업공간의 정규 경로가 다릅니다. 저장된 파일 경로는 읽지 않습니다. |
| `not_recorded` | 이 기억에는 기준이 없습니다. 최신이라는 뜻이 아닙니다. |

고른 파일마다 `unchanged`, `changed`, `missing`, `unavailable` 결과가 따로 나옵니다. 관련 없는 파일은 비교하지 않습니다. Git HEAD만 바뀌었다고 파일 비교 상태가 바뀌지는 않습니다.

## 저장되는 내용

선택 항목인 `metadata.code_evidence`에는 다음이 들어갑니다.

- 기존 작업공간 식별 함수로 정규 경로에서 만든 작업공간 ID
- 로컬 기록 시각
- 가능하면 Git HEAD(참고용)
- 최대 16개의 상대 파일 경로와 SHA-256 해시

지문은 커밋하지 않은 변경까지 포함한 작업 파일을 나타냅니다. HEAD가 있다고 그 파일이 커밋과 같다거나 거기서 테스트가 돌았다는 뜻은 아닙니다. 파일 내용은 이 필드에 저장하지 않습니다. 그래도 경로와 해시로 정보가 드러날 수 있으니, 해시를 비밀 저장으로 여기지 마세요.

경로는 최대 256 UTF-8 바이트, 파일은 최대 1 MiB입니다. 절대 경로, 상위 폴더 이동, Git 내부 파일, 자격 증명처럼 보이는 경로, 심볼릭 링크, 일반 파일이 아닌 것은 거부합니다. 파일은 로컬 CLI에서만 읽습니다. HTTP/MCP 서버는 메타데이터를 검사하고 저장할 뿐, 참조된 파일을 읽지 않습니다.

이것은 믿을 수 있는 로컬 작업 컴퓨터를 전제로 한 기능이며, 다른 프로세스가 파일을 바꿔치기하는 것까지 막는 파일 시스템 격리는 아닙니다. 기업용 접근 제어도 아닙니다. 다른 클론이나 다른 컴퓨터는 작업공간 경로가 달라서 일부러 자동 비교하지 않습니다.

## 기존 도구와 대체

저장된 기록은 일반 검색 대상 기억입니다. `GET /chunk/{id}`와 MCP `memory_get`은 `code_evidence`를 `caller_recorded_fingerprints_not_solution_verification`라는 표시와 함께 돌려줍니다. MCP `memory_remember`도 같은 제한된 객체를 받지만, 호출자가 넣은 해시를 서버가 따로 검증하지는 않습니다. pi 0.4.0에서는 `memory_remember(code_files=["relative/file"])`가 로컬 기준을 명시적으로 기록하고, `memory_get(check_code=true)`가 명시적으로 비교합니다. 둘 다 자동이 아닙니다. `memnest evidence capture`는 네트워크를 쓰지 않습니다. `memnest evidence compare`는 표준 입력으로 제한된 기준을 받으므로, 원격 저장소의 기준도 서버가 파일을 읽지 않고 로컬에서 확인할 수 있습니다.

보고나 기준을 바꾸려면 `--supersedes "$OLD_MEMORY_ID"`로 새 보고를 저장합니다. 같은 프로젝트 확인 규칙이 그대로 적용됩니다. 이전 기록은 ID로는 볼 수 있지만 일반 검색에서는 숨겨집니다. 문서를 제자리에서 바꾸거나 기준만 고치는 요청은 거부합니다. 중요도 같은 일반 수정은 기준을 유지합니다.

기존 기록에 표시를 새로 붙이거나 소급해서 기준을 만들지는 않습니다. 기본 검색, 자동 회상, 대화 수집은 그대로입니다. 실행 검증은 Linux x86_64에서 했고, macOS와 Windows 네이티브의 경로 처리는 시험하지 않았습니다.

## 실행 회귀 검사와 한계

```sh
cd /path/to/memnest
(cd core && cargo build --offline && cargo test --offline --lib code_evidence::tests)
python3 scripts/test-code-evidence.py
```

스크립트는 일회용 Git 작업공간과 인증된 루프백 서비스를 쓰고, 기존 `core/target/test-model-cache`를 사용합니다. 실제 운영 기억 DB는 읽지 않습니다. 실제 CLI 저장과 확인, HTTP와 MCP 읽기/쓰기, 고른 파일 수정, 관련 없는 Git 커밋, 작업공간 불일치, 기준 없는 옛 기록, 대체, 제자리 수정 거부, 인증, 상위 폴더 이동 거부, 없는 파일, 심볼릭 링크 거부를 확인합니다.

이 검사는 약속한 동작과 좁은 파일 비교 동작을 확인할 뿐입니다. 코딩 실수 감소, 토큰 비용 감소, 더 나은 검색, 설치 개선, Hindsight나 memU 같은 다른 제품보다 낫다는 점을 보여 주지는 않습니다. 별도의 [로컬 제품 비교](local-provider-comparison.ko.md)는 작은 고정 자료와 로컬 모델 조합을 다루며, 파일 확인이 코딩 결과를 개선한다는 증거가 아닙니다.
