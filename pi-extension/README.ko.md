# pi-memnest

<!-- markdownlint-disable MD013 -->

[English](README.md) | [한국어](README.ko.md)

[pi](https://github.com/badlogic/pi-mono)를 실행 중인 [memnest](https://github.com/Blue-B/memnest) 서비스에 연결해서, 기억 도구 5개와 작업 카드를 줍니다. 기억 엔진은 이 패키지에 없으니 코어를 먼저 실행하세요.

## 설치

Node.js 20 이상, pi, memnest 서비스(보통 `http://127.0.0.1:3111`)가 필요합니다.

```bash
pi install npm:pi-memnest@0.5.0
```

로컬 경로나 다른 버전으로 Memnest가 이미 등록돼 있다면, 도구가 겹치지 않게 그 항목을 먼저 지우세요. 고정한 버전은 `pi update --extensions`로 바뀌지 않습니다.

다른 주소를 쓰면 `MEMNEST_URL`을, 코어가 토큰을 요구하면 `MEMNEST_TOKEN`을 설정합니다. 연결은 `/memnest`로 확인합니다. 서비스 상태, 기억 개수, 데이터 폴더, 자동 회상 여부, 진행 중인 작업 카드, 이 세션이 기록 중인 카드를 보여 줍니다.

## 도구

| 도구 | 용도 |
| --- | --- |
| `memory_remember` | 기억을 저장합니다. 종류는 `record`, `fact`, `rule`, `procedure`이고, `supersedes`로 이전 기억을 대체합니다. |
| `memory_search` | 현재 작업공간과 `playbook`을 검색합니다. `project=all`이면 전체를 검색합니다. |
| `memory_get` | 기억을 나눠 읽고(`offset`, `max_chars`), 주변 대화도 함께 봅니다(`before`, `after`). |
| `memory_update` | 기억 하나를 고칩니다. |
| `memory_delete` | 기억 하나를 휴지통으로 옮깁니다. |
| `task_card` | 작업 카드를 저장, 목록, 열기, 닫기, 끝내기 합니다. |
| `secret_set`, `secret_get`, `secret_list`, `secret_delete` | 암호화 금고입니다. pi를 시작하기 전에 `MEMNEST_EXPOSE_SECRET_TOOLS=1`을 설정해야 켜집니다. |

```text
memory_remember text="Project X deploys on port 8320" memory_kind="fact"
memory_search query="Project X deployment port"
memory_remember text="Project X now deploys on port 8420" memory_kind="fact" supersedes="manual_..."
```

`project`를 빼면 pi의 작업 폴더를 보내고, 코어가 그 폴더 전용 작업공간으로 정합니다. 일반 기억은 암호화되지 않으니, 금고 도구는 평문 자격 증명을 맡겨도 되는 에이전트에서만 켜세요. 처음 검색할 때는 임베딩 모델을 내려받느라 시간이 더 걸립니다.

## 작업 카드

며칠 이어지는 작업의 인계 문서입니다. 따로 켜지 않아도 동작합니다.

```text
task_card action=save title="blog migration" text="# Goal ..."   # 같은 제목이면 이전 카드를 대체
task_card action=list text="images"                              # 주제로 찾기. text를 빼면 최신순
task_card action=get title="blog migration"                      # 열고 이 세션 동안 고정
task_card action=close title="blog migration"                    # 다른 일로 넘어가면 기록 중단
task_card action=done title="blog migration"                     # 끝내기. 다시 저장하면 재개
```

세션 첫 요청에 진행 중인 카드를 최대 20개 보여 줍니다. 연 카드는 시스템 프롬프트에 고정되고, 사용자가 보내는 말은 모두 원문 그대로 카드에 붙어서 요약 과정에서 빠지지 않습니다. 실제 사용에서 모델이 카드를 놓친 경우(카드가 아직 없을 때, 압축됐을 때, 카드 없이 `memory_remember`를 쓸 때, 연 카드를 메시지 8개 동안 저장하지 않을 때)에는 한 번씩 짧게 알려 줍니다. 카드는 코어를 통해 Claude Code, Codex, 다른 MCP 도구와 같이 씁니다. [동작 방식, 측정, 한계](https://github.com/Blue-B/memnest/blob/main/docs/task-cards.ko.md)

## 명령

pi 터미널 화면에서 모델 없이 실행되고, 보여 주는 내용은 모델에 전달되지 않습니다.

- `/memnest`: 서비스 상태, 기억 개수, 작업 카드, 이 세션이 기록 중인 카드.
- `/memnest-cards`: 진행 중인 작업 카드(최신순, 이 세션이 기록 중인 카드는 `[open]`)와 끝낸 카드를 보여 줍니다. 카드를 고르면 글을 읽고, 이 세션에서 열거나 닫고, 끝냄으로 표시할 수 있습니다(먼저 확인을 묻습니다).
- `/memnest-search [project:<이름>|project:all] <질문>`: `memory_search`와 같은 검색입니다. 프로젝트, 날짜, 종류, 점수, 발췌가 있는 결과를 최대 10개 보여 주고, 하나를 고르면 전문을 읽습니다. `project:`를 쓰지 않으면 현재 폴더의 작업공간에서 찾습니다. `project:` 뒤의 이름은 자동 완성됩니다. 질문이 없으면 물어봅니다.

화면이 없는 실행(`pi -p`)에서는 글만 출력하고 아무것도 묻지 않습니다. 코어가 꺼져 있으면 한 줄로 알려 줍니다.

## 자동 회상

기본으로 꺼져 있습니다. 요청마다 관련 기억을 넣으려면 pi를 시작하기 전에 `MEMNEST_AUTOCONTEXT_MODE=balanced`를 설정합니다. 넣는 내용에는 기억 ID가 붙어서 에이전트가 `memory_get`으로 원문을 확인할 수 있고, 가져온 글은 지시가 아니라 참고 자료로 다룹니다.

| 변수 | 기본값 | 용도 |
| --- | --- | --- |
| `MEMNEST_AUTOCONTEXT_MODE` | `off` | `balanced`나 `aggressive`면 켜집니다. |
| `MEMNEST_AUTOCONTEXT_DISABLE` | 없음 | `1`이면 모드와 상관없이 끕니다. |
| `MEMNEST_AUTOCONTEXT_TOP` | `2` | 한 번에 넣는 결과 수 |
| `MEMNEST_AUTOCONTEXT_MAX_INJECTIONS` | `4` | 세션당 넣는 횟수 |
| `MEMNEST_AUTOCONTEXT_MIN_SCORE` | `0.25` | 넣을 최소 점수 |
| `MEMNEST_AUTOCONTEXT_EXCLUDE` | `_superseded,default,root,global` | 회상에서 빼는 컬렉션 |
| `MEMNEST_AUTOCONTEXT_N` | `20` | 순위를 매기기 전에 가져올 후보 수 |
| `MEMNEST_AUTOCONTEXT_MIN_LEN` | `16` | 이보다 짧은 요청은 건너뜀 |
| `MEMNEST_AUTOCONTEXT_DOC_CHARS` | `240` | 결과당 남길 글자 수 |
| `MEMNEST_AUTOCONTEXT_TIMEOUT_MS` | `1500` | 이 시간이 지나면 기억 없이 진행 |

## 대화 수집

이 확장은 대화를 직접 저장하지 않습니다. 코어의 `memnest watch`가 pi, Claude Code, Codex 대화 기록을 수집해서 두 번 저장되지 않습니다. [대화 수집과 과거 기록 가져오기](https://github.com/Blue-B/memnest/blob/main/docs/history-import.ko.md)를 보세요.

## 개발

```bash
npm install      # pi가 불러오는 dist/index.mjs도 빌드
npm run smoke
npm run e2e      # 코어 출시용 빌드가 필요
```

[변경 이력](./CHANGELOG.ko.md), [제3자 고지](./THIRD_PARTY_NOTICES.md)

## 라이선스

MIT © Blue-B
