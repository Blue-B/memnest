# memnest

<!-- markdownlint-disable MD013 MD033 -->

<img src="docs/logo.png" alt="memnest 로고" width="220">

[English](README.md) | [한국어](README.ko.md)

코딩 AI는 세션이 끝나면 다 잊어버립니다. Memnest를 쓰면 전에 무엇을 왜 결정했는지 AI가 다시 찾아볼 수 있습니다. AI가 쓴 요약이 아니라 당시 대화 원문에서 찾습니다.

- **요약이 아니라 원문입니다.** 저장과 검색에 모델 API를 쓰지 않아서 API 키도, 추가 토큰 비용도 없습니다. 모든 결과에 열어볼 수 있는 출처 ID가 붙습니다.
- **코딩 AI 여러 개가 기억 하나를 같이 씁니다.** Claude Code, Codex, Cursor, pi가 같은 로컬 서비스를 씁니다. Claude Code에서 정한 내용을 Codex에서 찾을 수 있습니다.
- **내 컴퓨터에 남습니다.** Rust 실행 파일 하나에 SQLite와 로컬 다국어 임베딩 모델로 동작합니다.

[![Latest release](https://img.shields.io/github/v/release/Blue-B/memnest?label=release)](https://github.com/Blue-B/memnest/releases/latest)
[![npm: pi-memnest](https://img.shields.io/npm/v/pi-memnest?label=npm%20pi-memnest&color=cb3837)](https://www.npmjs.com/package/pi-memnest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

## 설치

```bash
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/main/core/scripts/install.sh | bash
```

Windows에서는 PowerShell에서 실행합니다(관리자 권한이 필요 없습니다).

```powershell
irm https://raw.githubusercontent.com/Blue-B/memnest/main/core/scripts/install.ps1 | iex
```

서버와 대화 수집기를 시작하고, 찾은 Claude Code, Codex, Cursor 설정에 Memnest를 연결합니다. 기존 설정은 먼저 백업하고 복구 명령을 출력합니다. Codex는 새 훅을 `/hooks`에서 한 번 신뢰해야 실행합니다.

Apple Silicon macOS, systemd가 있는 Linux(x86_64는 glibc 2.35 이상, Arm64는 glibc 2.39 이상), 또는 64비트 x86 Windows 10/11과 Python 3이 필요합니다. Windows에서 Python은 클라이언트 연결에만 쓰이고, Windows 압축 파일은 v0.5.1 다음 릴리스부터 제공됩니다([자세히](docs/operations.ko.md#windows)). 처음 저장하거나 검색할 때 약 1.1GB 임베딩 모델을 내려받습니다.

- pi: `pi install npm:pi-memnest@0.5.0`을 실행하고 `/memnest`로 확인합니다.
- 그 밖의 MCP 도구: `http://127.0.0.1:3111/mcp`에 연결합니다([연결 예시](adapters/README.ko.md)).

## 사용법

AI에게 평소 말투로 말하면 됩니다. 어떤 도구를 쓸지는 AI가 고릅니다.

| 이렇게 말하면 | 이렇게 됩니다 |
|---|---|
| "작업 관리 도구를 바꾼 이유가 뭐였지? 그때 논의를 찾아줘." | `memory_search`로 기록을 찾고 `memory_get`으로 원문을 연 뒤, 확인할 수 있는 출처 ID와 함께 답합니다 |
| "기억해 둬. 배포는 release 브랜치에서만 하고 금요일에는 안 해." | `memory_remember`가 말한 그대로 저장합니다. 연결된 다른 AI도 나중에 찾을 수 있습니다 |
| "그건 옛날 얘기야. 스테이징 포트는 이제 8443이야. 기억을 고쳐 줘." | `memory_update`가 새 내용으로 바꾸고 이전 내용은 이력에 남깁니다 |
| "결제 웹훅 결정을 모든 프로젝트에서 찾아줘." | 평소에는 지금 폴더의 작업공간과 공통 `playbook`만 찾고, 이렇게 말하면 전체를 찾습니다 |
| "이 마이그레이션 작업 카드 만들어 줘." 다음 날 "마이그레이션 카드 이어서 하자." | `task_card`가 계획과 사용자 말을 저장하고, 다음 세션이 카드를 열어 이어 갑니다 |

검색 결과는 저장된 글 그대로이고 프로젝트, 점수, ID가 붙습니다.

```text
[1] project=shop-api score=0.2531 id=manual_5ea65b71... doc_len=96
    배포는 release 브랜치에서만 하고 금요일에는 하지 않는다. 금요일 롤백 뒤 2026-10-02에 결정.
hint: stronger matches exist outside this workspace scope: billing (2). Search again with project="billing" (or project="all").
```

`hint` 줄은 지금 폴더보다 다른 프로젝트에 확실히 더 맞는 기억이 있을 때 나옵니다. 프로젝트 이름과 개수만 알려 주고 내용은 보여 주지 않습니다.

도구: `memory_search`, `memory_get`, `memory_remember`, `memory_update`, `memory_delete`, `task_card`

[![한 클라이언트가 저장한 결정을 다른 클라이언트가 같은 기록으로 찾아온 결과](docs/demo-result.ko.png)](docs/demo.ko.md)

## 동작 구조

```text
 Claude Code      Codex      Cursor      pi        ChatGPT, Claude, Grok (web)
   MCP + hook   MCP + hook    MCP     extension    read-only gateway (optional)
       |            |          |          |                    |
       +------------+----------+----------+--------------------+
                                |
                 memnest server (one Rust binary, 127.0.0.1:3111)
                   save       text stored as written, embedded locally
                   search     keyword (BM25) + vector, fused and ranked
                   context    memories on the prompt's topic, for the hook
                   task cards, vault
                                |
                 ~/.memnest
                   memory.db    SQLite, the source of truth
                   text_index   keyword index (Tantivy)
                   vectors      vector index
                   models       multilingual-e5-base, runs on the CPU
                   master.key   key for the vault (AES-256-GCM)

 memnest watch: reads the session files of Claude Code, Codex and pi
                and stores each visible turn through the same server
```

1. **저장.** 기억은 쓴 그대로 저장합니다. 고쳐 쓰거나 요약하지 않습니다. 작은 로컬 모델이 글을 숫자 묶음(임베딩)으로 바꿔 두어서 나중에 뜻으로 찾을 수 있습니다.
2. **검색.** 같은 질문으로 두 가지 검색을 돌립니다. 키워드 검색은 같은 낱말(파일 이름, 숫자, 정확한 용어)을 찾고, 벡터 검색은 표현이 달라도 뜻이 같은 글을 찾습니다. 두 순위를 합친 뒤, 일부러 저장한 기억은 대화 기록보다 위로, 오래된 것은 아래로 보내고, 거의 같은 글은 솎아 냅니다. 기본으로 3개를 돌려주고 결과마다 ID가 붙습니다.
3. **자동 회상.** 프롬프트 훅을 쓰면 질문을 보낼 때 memnest가 기억을 찾아, 그 질문의 주제에 맞는 것만 붙여 줍니다. 막연한 질문에는 아무것도 붙이지 않습니다.
4. **범위.** 작업하는 폴더마다 작업공간이 따로 있고 `playbook`은 모두가 같이 씁니다. 자격 증명은 암호화 금고에 들어가고 검색에는 나오지 않습니다.

이 단계 어디에서도 언어 모델이 돌지 않습니다. 그래서 저장과 검색에 API 키가 필요 없습니다. 이렇게 만든 이유는 [설계 결정](docs/design-decisions.ko.md)에, 검색 순위와 측정 결과는 [벤치마크](docs/benchmarks.ko.md)에 있습니다.

## 작업 카드

며칠 걸리는 작업에 씁니다. 카드 본문은 AI가 쓰고, 사용자 말은 Memnest가 원문 그대로 붙이며, 다음 세션은 이 카드를 열어 이어 갑니다. 5일 동안 이어 쓴 실제 pi 세션을 다시 돌려 보니, 답 한 번에 읽은 맥락이 321K 토큰에서 174K 토큰으로 줄었습니다. 블라인드 채점에서 사용자 방침을 지킨 경우는 카드가 있을 때 3번 중 3번, 없을 때 3번 중 0번이었습니다. 카드가 있다고 모델이 반드시 따르는 것은 아닙니다. [동작 방식과 한계](docs/task-cards.ko.md)

## pi 명령

pi에서 직접 쳐서 memnest를 들여다보는 명령입니다. 모델을 거치지 않고, 모델의 맥락에 아무것도 넣지 않습니다.

| 명령 | 하는 일 |
|---|---|
| `/memnest` | 서비스 상태, 기억 개수, 작업 카드, 이 세션이 기록 중인 카드 |
| `/memnest-cards` | 작업 카드를 최근 순으로 보여 줍니다. 하나를 고르면 내용을 읽고, 이 세션에 열거나 닫고, 완료 처리할 수 있습니다 |
| `/memnest-search <질문>` | AI가 하는 것과 같은 검색을 직접 합니다. 결과는 10개까지 나오고, 고르면 전체 글이 보입니다. 앞에 `project:<이름>`이나 `project:all`을 붙이면 범위가 바뀝니다 |
| `/memnest-recent` | 일부러 저장한 기억 가운데 최근 20개(대화 기록 제외) |
| `/memnest-review` | 저장된 규칙과 선호를 확인한 지 오래된 것부터 하나씩 보여 줍니다. 유지, 삭제(휴지통으로 가고 30일 안에 복구 가능), 건너뛰기, 중단 가운데 고릅니다 |

`/memnest-review`가 있는 이유는 옛 규칙이 지금도 맞는지 memnest는 알 수 없고 사용자는 알기 때문입니다. `/memnest-...` 명령 네 개와 `hint` 줄, 주제에 맞는 것만 붙이는 자동 회상은 `develop` 브랜치에 있고, 코어 0.5.1과 pi-memnest 0.5.0 다음 릴리스에 들어갑니다.

## 그 밖의 기능

대화 수집(`memnest watch`), [과거 기록 가져오기](docs/history-import.ko.md), [코드 근거](docs/code-evidence.ko.md), 자격 증명용 암호화 금고, [팀 권한](docs/team-access.ko.md), ChatGPT와 Claude, Grok용 [웹 커넥터](docs/web-connectors.ko.md)가 있습니다. 자동 회상은 직접 켜야 동작합니다.

## 알아둘 점

- 검색이 기록을 놓칠 수 있고, 옛 결정이 지금도 맞는지는 판별하지 못합니다. 언어 모델 없이 공개 시험 두 가지로 재면, 답이 들어 있는 대화가 처음 5개 결과 안에 나오는 비율이 96%이고(LongMemEval-S, 표본 질문 200개. 배포된 0.5.1은 93.5%), 답이 들어 있는 바로 그 한 마디가 처음 10개 안에 나오는 비율이 77%입니다(LoCoMo. 0.5.1은 70%). 기억을 찾았는지만 센 것이고, AI가 그걸로 옳게 답했는지는 아닙니다. [무엇을 시험했고 어떻게 읽는지](docs/benchmarks.ko.md)
- 보이는 대화 텍스트만 수집합니다. 추론 내용, 도구 출력, 이미지는 가져오지 않습니다.
- AI가 읽은 기억은 그 AI 제공자에게 전달됩니다. 일반 기억은 암호화되지 않으니 3111 포트를 인터넷에 열지 마세요. 웹 커넥터 URL은 비밀번호처럼 다뤄야 합니다.
- 삭제한 기록은 휴지통에 30일 남고, 백업에는 복사본이 남을 수 있습니다. [보안과 삭제 범위](SECURITY.ko.md)

## 문서

[운영, 업그레이드, 백업](docs/operations.ko.md), [변경 이력](core/CHANGELOG.ko.md), [개발 안내](CONTRIBUTING.ko.md)

## 라이선스

MIT © Blue-B
