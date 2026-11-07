# pi-memnest

<!-- markdownlint-disable MD013 -->

[English](README.md) | [한국어](README.ko.md)

pi용 로컬 memnest 연결 확장입니다.

`pi-memnest`는 [pi](https://github.com/badlogic/pi-mono)를 실행 중인 [memnest](https://github.com/Blue-B/memnest) HTTP 서비스에 연결합니다. pi에 기본 기억 도구 5개와 작업 카드를 줍니다. 비밀 도구 4개는 직접 켜야 합니다. 자동 기억 카드(자동 회상)는 기본으로 꺼져 있고, 기억 도구는 그것 없이도 동작합니다.

이 확장에는 기억 엔진이 들어 있지 않습니다. 설치하기 전에 Rust 코어를 먼저 실행하세요.

## 필요 환경

- Node.js 20 이상
- pi
- memnest HTTP 서비스. 보통 `http://127.0.0.1:3111`

## 설치

[memnest 코어 서비스](https://github.com/Blue-B/memnest/blob/main/README.ko.md#설치)를 시작한 뒤, 맞는 확장을 설치합니다.

```bash
pi install npm:pi-memnest@0.4.0
```

로컬 경로나 다른 고정 버전으로 Memnest가 이미 등록돼 있다면, 도구가 중복되지 않게 그 패키지 선언을 먼저 지우세요. 고정한 패키지는 `pi update --extensions`로 바뀌지 않습니다. 코어와 확장 버전을 맞춰 두세요.

소스 체크아웃으로 개발하려면:

```bash
cd /path/to/memnest/pi-extension
npm install
pi install .
```

확장은 `MEMNEST_URL`이 있으면 그 주소를, 없으면 `http://127.0.0.1:3111`에 연결합니다. 코어가 bearer 인증을 요구하면 `MEMNEST_TOKEN`을 설정하세요.

pi에서 연결을 확인합니다.

```text
/memnest
```

이 명령은 서비스 상태, 기억 개수, 사용 중인 데이터 폴더, pi 자동 회상이 켜져 있는지, 진행 중인 작업 카드와 마지막 저장 시각, 이 세션이 내 말을 기록하고 있는 카드를 보여 줍니다.

## 도구

확장은 기억 도구 5개와 `task_card`를 등록합니다. pi를 시작하기 전에 `MEMNEST_EXPOSE_SECRET_TOOLS=1`을 설정하면 금고 도구 4개도 등록합니다. `/memnest` 명령과 자동 회상 훅은 모델이 쓰는 도구 목록에 상태나 관리 도구를 더하지 않고 동작합니다.

| 도구 | 용도 |
| --- | --- |
| `memory_remember` | 지속되는 기억을 저장합니다. 민감하다고 표시된 값은 거부하며, `secret_set`을 써야 합니다. |
| `memory_search` | 기본으로 현재 작업공간을 검색하고, `project=all`을 명시하면 전체를 검색합니다. |
| `memory_get` | 기억 한 쪽과, 선택적으로 같은 대화 기록의 주변 내용을 읽습니다. |
| `memory_update` | 기억 하나를 고치고 색인을 갱신합니다. |
| `memory_delete` | 기억 하나를 내부 휴지통으로 옮깁니다. |
| `task_card` | 여러 세션에 걸친 작업의 인계 카드를 저장, 목록, 열기, 닫기, 끝내기 합니다. |
| `secret_set`, `secret_get`, `secret_list`, `secret_delete` | 직접 켜는 AES-256-GCM 금고 도구입니다. 암호화를 쓸 수 없으면 금고 작업은 실패로 끝납니다. |

일반 기억은 저장할 때 암호화되지 않습니다. 평문 자격 증명을 맡겨도 되는 에이전트에서만 비밀 도구를 켜고, 코어 데이터 폴더와 `master.key`는 비공개로 두세요.

## 기본 사용

```text
memory_remember text="Project X deploys on port 8320" memory_kind="fact" confidence=1
memory_search query="Project X deployment port"
memory_remember text="Project X now deploys on port 8420" memory_kind="fact" supersedes="manual_..."
memory_delete id="manual_..."
```

`project`를 빼면 확장은 pi의 절대 경로 `cwd`를 보내고, 코어는 개인 작업공간 ID를 만들어 회상에 `playbook`을 포함합니다. 코어는 색인 작업을 SQLite에 안전하게 쌓고, Tantivy와 HNSW가 맞춰진 뒤에만 저장 완료를 알립니다. 똑같은 일반 기록은 기존 ID를 다시 쓸 수 있습니다. 구조화된 사실, 규칙, 출처, 정정은 의미 기반 중복 제거를 하지 않습니다. 임베딩이 처음 필요한 작업은 코어가 임베딩 모델을 그때 내려받기 때문에 더 오래 걸립니다.

## 구조화된 기억

`memory_remember`는 이식 가능한 네 종류를 지원합니다.

- `record`: 지난 대화와 결과
- `fact`: 안정적인 프로젝트나 환경 정보
- `rule`: 선호, 결정, 지켜야 할 선
- `procedure`: 다시 쓸 수 있는 검증된 작업 절차

선택 필드인 `confidence`, `source_ids`, `supersedes`는 데이터 구조를 pi에 묶지 않고 출처를 남깁니다. `supersedes`는 같은 작업공간의 활성 기억을 한 번에 숨깁니다. 신뢰도는 클라이언트의 주장이며, 순위를 자동으로 올리지 않습니다.

## 자동 회상

자동 회상은 기본으로 꺼져 있습니다. 켜지 않으면 프롬프트 시점 검색이나 카드를 등록하지 않으며, `memory_remember`, `memory_search`, `memory_get`, 대화 수집은 그대로 동작합니다. 이렇게 해서 연결 확인이나 기억이 필요 없는 요청이 자동 회상으로 맥락을 쓰지 않게 합니다.

켜려면 pi를 시작하기 전에 `MEMNEST_AUTOCONTEXT_MODE=balanced`를 설정합니다. 기존 값 `aggressive`도 같은 의미 기준을 켭니다. `off`, `none`, 설정 안 함, 알 수 없는 값은 회상을 끈 상태로 둡니다. `MEMNEST_AUTOCONTEXT_DISABLE=1`은 켜진 모드보다 우선해서 끕니다.

켜면 의미 있는 프롬프트마다, 세션당 카드 제한에 닿을 때까지 현재 작업공간과 `playbook`을 검색합니다. 짧은 답, 슬래시 명령, 똑같이 반복된 프롬프트, 점수가 낮은 결과에는 아무것도 넣지 않습니다. 언어별 키워드 목록 없이 원래 프롬프트로 검색합니다. 점수가 높아도 관련 없을 수 있습니다. 이 기준은 기억이 필요한지, 저장된 사실이 아직 맞는지를 판단하지 않습니다.

모든 카드에는 기억마다 ID, 프로젝트, 생성 시각이 들어갑니다. 전체 텍스트는 그 ID로 `memory_get`을 불러 확인하세요. 생성 시각은 확인한 시각이 아닙니다. 쓸 수 있는 ID, 프로젝트, 유한한 숫자 점수가 없는 결과는 기본 높은 점수를 주지 않고 버립니다. 모든 카드는 가져온 텍스트를 믿을 수 없는 참고 자료라고 표시합니다. 자동 맥락은 일부러 저장했거나 정리된 기억만 받고, 수집된 대화 기록은 예전 대화를 묻는 명시적 `memory_search`로만 찾습니다. 저장된 텍스트 안의 마크업은 넣기 전에 이스케이프하며, 에이전트는 기억 안의 명령을 따르지 말고 주장을 확인해야 합니다.

자주 쓰는 설정:

| 변수 | 기본값 | 용도 |
| --- | --- | --- |
| `MEMNEST_AUTOCONTEXT_MODE` | `off` | `balanced`와 `aggressive`만 의미 기반 회상을 켭니다. |
| `MEMNEST_AUTOCONTEXT_TOP` | `2` | 카드 하나에 넣는 최대 결과 수 |
| `MEMNEST_AUTOCONTEXT_MAX_INJECTIONS` | `4` | 세션 하나의 최대 카드 수 |
| `MEMNEST_AUTOCONTEXT_MIN_SCORE` | `0.25` | 카드에 넣을 최소 점수 |
| `MEMNEST_AUTOCONTEXT_EXCLUDE` | `_superseded,default,root,global` | 자동 검색에서 뺄 컬렉션 |
| `MEMNEST_AUTOCONTEXT_DISABLE` | 없음 | `1`이면 검색을 끕니다. `MODE=off`와 같은 효과 |
| `MEMNEST_AUTOCONTEXT_N` | `20` | 순위를 매기기 전에 서비스에 요청할 후보 수 |
| `MEMNEST_AUTOCONTEXT_MIN_LEN` | `16` | 이보다 짧은 프롬프트는 무시 |
| `MEMNEST_AUTOCONTEXT_DOC_CHARS` | `240` | 카드에 남길 결과당 글자 수 |
| `MEMNEST_AUTOCONTEXT_TIMEOUT_MS` | `1500` | 카드 없이 진행하기 전까지 검색에 쓸 시간 |

### 긴 기억 읽기

`memory_get(id="...", offset=0, max_chars=2000, before=1, after=1)`은 다음 위치, 출처 정보, 선택적인 주변 대화 내용을 JSON으로 돌려줍니다. 검색은 여전히 결과마다 최대 600자를 돌려주며, 검색 전체 합계 제한은 없습니다. 서버 쪽 페이지 나누기와 주변 내용을 쓰려면 코어와 확장을 함께 업그레이드하세요. 예전 코어에서는 pi가 로컬에서 텍스트를 자르고 `paging: "client"`로 표시하며, `before`나 `after`가 0이 아니면 업그레이드 오류를 돌려줍니다. `id`만 넘기는 호출은 계속 호환됩니다.

글자 수 예산과 수집 순서 한계는 [제한된 원문 조회](https://github.com/Blue-B/memnest/blob/main/docs/bounded-retrieval.md)(영문)를 보세요.

## 작업 카드

`task_card`는 여러 세션이나 며칠에 걸쳐 이어지는 작업의 인계 카드를 관리합니다. 항상 등록됩니다. 코어도 같은 카드를 Claude Code, Codex, 다른 MCP 클라이언트에 제공하므로, pi에서 저장한 카드를 거기서도 열 수 있습니다. 이 확장은 예전 코어에서도 동작하도록 자체 구현을 따로 갖고 있습니다.

```text
task_card action=save title="blog migration" text="# Goal ..."   # 같은 제목이면 이전 카드를 대체
task_card action=list text="images"                              # 주제로 찾기. text를 빼면 최신순
task_card action=get title="blog migration"                      # 이 세션 동안 카드를 고정
task_card action=close title="blog migration"                    # 다른 일로 넘어가면 기록 중단
task_card action=done title="blog migration"                     # 끝난 카드 보관함으로 이동. 다시 저장하면 재개
```

카드는 공용 `task-cards` 컬렉션에 있는 일반 Manual 기억이라 어느 폴더에서든 보입니다. 끝난 카드는 `task-cards-done`으로 옮겨져서, 세션 시작 검색은 진행 중인 작업만 훑습니다. 최신판이 이전 판을 대체하고, 이전 판은 기록으로 남습니다. 세션의 첫 프롬프트로 카드를 검색해서, 진행 중인 제목을 관련 있는 순서, 그다음 최신순으로 최대 20개 보여 줍니다. 카드 본문은 열기 전까지 읽지 않습니다. 카드가 열려 있는 동안 사용자의 모든 메시지를 원문 그대로 카드에 붙입니다. 모델의 저장은 자기 요약만 바꾸므로, 약한 모델이라도 사용자 말을 빠뜨리거나 뒤집을 수 없습니다. 연 카드는 최대 3장까지 매 요청 시스템 프롬프트에 고정되며, 사용자가 말한 방침이 모델 자신의 분석보다 우선한다는 고정 문장이 함께 붙습니다. 그래서 압축으로 요약되지 않고, 압축 뒤 다음 프롬프트에서 바뀐 내용을 저장하라고 알려 줍니다. 고정한 내용은 처음 고정할 때와 압축 뒤에만 바뀌어서, 요청 사이에 프롬프트 캐시를 깨지 않습니다. 이어서 연 세션은 열려 있던 카드를 다시 열고, 12,000자가 넘는 카드는 거부합니다. 카드가 없으면 세션 시작 검색 뒤에 언제 카드를 저장할지 한 줄로 알려 주며, 코어가 1.5초 안에 응답하지 않으면 검색을 건너뜁니다. 제목 20개 목록은 약 1,000자입니다.

실제 5일짜리 세션을 다시 돌려 보니, 그 답에 읽은 맥락이 321,213토큰에서 174,118토큰으로 줄었습니다. 세션 전체로 계산하면, 매일 카드로 새 세션을 시작했을 때 맥락 토큰이 39% 줄었습니다. 규칙 문장과 함께 고정한 카드는 다시 돌린 실패 장면을 블라인드 채점 3번 모두 막았고, 이전에는 3번 모두 실패했습니다. [측정과 한계](https://github.com/Blue-B/memnest/blob/main/docs/task-cards.ko.md)

## 자동 대화 수집

이 확장은 AutoLog 이벤트 훅을 설치하지 않습니다. pi 대화가 두 번 저장되지 않도록 `memnest watch` 하나로 수집하세요. watch는 가려진 사용자와 어시스턴트 대화를 요약 없이 저장하고, 긴 대화는 자르지 않고 순서 있는 조각으로 나누며, 새로 식별된 대화 AutoLog는 직접 지울 때까지 보관합니다. 예전 AutoLog는 코어의 보존 정책을 따릅니다.

## pi 밖에서

MCP는 호스트의 세션 이벤트를 다루지 않습니다. 대신 코어가 호스트와 무관한 자동 동작을 제공합니다.

- `memnest hook`은 호스트의 훅 내용을 표준 입력으로 읽고, 그 호스트가 기대하는 형태로 작업 카드와 맥락 묶음을 돌려줍니다.
- `memnest watch`는 Claude Code, pi, Codex 대화 기록을 따라가며 보이는 대화를 저장하고, 호스트 확장 훅이 필요 없습니다.

[대화 수집과 과거 기록 가져오기](https://github.com/Blue-B/memnest/blob/main/docs/history-import.ko.md)를 보세요. pi 안에서는 이 확장이 같은 기억 도구 약속을 제공하고, 선택적으로 금고 도구 4개를 더하며, `/memnest` 명령을 제공합니다.

## 개발

```bash
npm install
npm run build
npm run smoke
```

`npm install`은 `prepare` 스크립트를 실행해 `dist/index.mjs`를 빌드합니다. pi는 `src/index.ts`가 아니라 이 번들 ESM 파일을 불러옵니다.

필요한 서비스가 돌고 있으면 MCP 전체 확인도 할 수 있습니다.

```bash
npm run e2e
```

## 관련 문서

- 엔진 설치, 수명 주기, 서비스 설치, 백업, 보안은 [memnest README](https://github.com/Blue-B/memnest/blob/main/README.ko.md)
- 확장 변경 내역은 [CHANGELOG.ko.md](./CHANGELOG.ko.md)
- 번들에 포함된 것은 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)(영문)

## 라이선스

MIT © Blue-B
