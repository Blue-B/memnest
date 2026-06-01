<div align="center">

# memnest

**AI 코딩 에이전트를 위한 계층형 영구 메모리 — 로컬, 암호화, 무료.**

하나의 Rust 엔진, MCP 브리지, git 기반 감사 레이어. 클라우드 없음, 호출당 비용 없음.

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-orange.svg)
![Protocol](https://img.shields.io/badge/interface-MCP%20%2B%20HTTP-blue.svg)

[English](./README.md) · [한국어](./README.ko.md)

<br/>

<img src="docs/dashboard.png" alt="memnest 대시보드" width="820" />

</div>

---

## 개요

memnest는 AI 에이전트를 위한 로컬 우선(local-first) 메모리 시스템입니다. 모든 데이터는
`~/.memnest/memory.db` 하나의 SQLite에 저장되며, 어떤 클라이언트를 쓰든 — Claude Code,
Claude Desktop, Cursor, Cline, Codex CLI, pi, `curl` — 같은 메모리를 읽고 씁니다. 계정도,
클라우드도 없고, 데이터는 사용자의 컴퓨터를 벗어나지 않습니다.

설계상 특정 에디터나 에이전트에 묶이지 않습니다. 엔진이 **stdio MCP** 서버와 **HTTP API**를
하나의 저장소 위에 함께 제공하기 때문입니다.

## 기능

- **하이브리드 검색** — BM25 전문검색(Tantivy)과 벡터 유사도(HNSW)를 융합하며, 네이티브 [fastembed](https://github.com/Anush008/fastembed-rs) 임베딩을 사용합니다.
- **컨텍스트 팩** — 항상 필요한 노트, 매칭되는 사실, 검색된 메모리를 한 번에 모아 프롬프트에 바로 넣을 수 있는 `<memnest_context>` 블록으로 반환합니다.
- **수정 가능한 메모리** — 기존 메모리를 id로 수정하고 텍스트/벡터 인덱스를 갱신하므로, 오래된 사실을 중복 추가하지 않고 바로 고칠 수 있습니다.
- **코어 노트** — persona, 사용자 프로필, 활성 프로젝트, 운영 규칙처럼 항상 필요한 작은 key-value 메모리 블록을 저장합니다.
- **지식 그래프 및 라이프사이클** — 메모리 간 관계를 저장하고, 중요도 가중 감쇠(decay)와 오래된 항목의 통합(consolidation)을 수행합니다.
- **암호화 비밀 보관소** — 자격증명을 AES-256-GCM(Argon2 파생 키)으로 저장하며, 입력 텍스트에서 비밀값(`sk-…`, 개인키, `api_key=…`)을 탐지해 저장 전에 마스킹합니다.
- **내장 대시보드** — 통합 검색, 컬렉션별 규모, 최근 입력을 한국어와 영어로 제공합니다.
- **하나의 저장소, 두 인터페이스** — HTTP API와 stdio MCP 서버가 같은 데이터베이스를 사용합니다.
- **안전한 기본값** — `127.0.0.1`에만 바인딩하고, 토큰 없이는 외부 바인딩을 거부하며, CSP·`nosniff`·`no-store` 헤더를 설정합니다.

## 지원 클라이언트

엔진이 stdio MCP를 사용하므로 MCP를 지원하는 모든 클라이언트와 동작합니다. 모두 하나의
`~/.memnest/memory.db`를 공유하므로, 한 클라이언트에서 기록한 메모리를 다른 모든
클라이언트에서 검색할 수 있습니다.

| 클라이언트 | 연결 방법 |
| ------ | -------------- |
| Claude Desktop, Cursor, Cline, Continue, Zed, opencode | `memnest --mcp` 명령 등록 — [`pi-extension/INSTALL-CLIENTS.md`](./pi-extension/INSTALL-CLIENTS.md) 참고. |
| Claude Code, Codex CLI, Kilo Code, Windsurf 등 | MCP를 지원하는 다른 클라이언트도 동일한 `memnest --mcp` 등록 사용. |
| pi | 한 줄 설치: `pi install npm:pi-memnest` (도구 + AutoLog 추가). |
| 스크립트 / 기타 | `http://127.0.0.1:3111` HTTP API 직접 호출. |

MCP 등록 방식은 어디서나 동일합니다:

```json
{ "command": "memnest", "args": ["--mcp"] }
```

## 저장소 구성

이 저장소는 시스템의 세 레이어를 담은 모노레포입니다.

| 디렉터리 | 패키지 | 언어 | 역할 |
| --------- | ------- | ---- | ---- |
| [`core/`](./core) | `memnest` | Rust | 엔진: HTTP API + stdio MCP 서버, 하이브리드 검색, 비밀 보관소, 대시보드. 필수. |
| [`pi-extension/`](./pi-extension) | `pi-memnest` | TypeScript | [pi](https://github.com/badlogic/pi-mono)용 편의 브리지(도구 + AutoLog). 다른 클라이언트는 코어에 MCP로 직접 붙으므로 선택 사항입니다. |
| [`journal/`](./journal) | `memnest-journal` | TypeScript | 데이터베이스를 git 마크다운 저장소로 미러링하여, 에이전트가 학습한 내용을 diff·revert·검토할 수 있게 하는 감사 레이어. 선택. |

## 빠른 시작

```bash
# 1. 엔진 빌드 및 실행
cd core
cargo build --release
./target/release/memnest          # 127.0.0.1:3111 에서 HTTP + 대시보드

# 2. 클라이언트 연결 (모든 MCP 클라이언트가 같은 명령 사용)
#    등록:  command "memnest", args ["--mcp"]
#    pi의 경우:
pi install npm:pi-memnest
memory_remember text="프로젝트 X는 8317 포트 사용"
memory_search   query="8317 포트"
memory_context  query="프로젝트 X 배포"
memory_update   id="manual_..." text="프로젝트 X는 이제 8320 포트 사용"

# 3. (선택) 메모리를 git으로 미러링
npm install -g memnest-journal
pjournal init ~/memory-journal && pjournal sync --push
```

대시보드는 `http://127.0.0.1:3111/` 에서 제공됩니다.

## 엔진 실행

```bash
memnest                      # HTTP 서버 + 대시보드 (127.0.0.1:3111)
memnest --mcp                # stdio MCP 모드
memnest --doctor             # 환경 및 저장소 상태 점검
memnest --warmup-embedding   # 임베딩 모델 사전 로드
```

자주 쓰는 옵션: `--port`, `--host`, `--data-dir`, `--backup-dir`, `--restore-dir`,
`--import-jsonl`. 전체 목록은 `memnest --help`.

## 사용 예시

연결된 클라이언트(여기서는 pi 도구)로 기록·검색:

```text
memory_remember text="배포는 8080 포트에서 블루-그린 방식 사용"
memory_search   query="배포 포트"
```

또는 HTTP API 직접 호출:

```bash
# 메모리 추가
curl -s http://127.0.0.1:3111/add \
  -H 'content-type: application/json' \
  -d '{"text":"배포는 8080 포트에서 블루-그린 방식","project":"acme"}'

# 하이브리드 검색 (BM25 + 벡터)
curl -s http://127.0.0.1:3111/search \
  -H 'content-type: application/json' \
  -d '{"query":"배포 포트","n_results":5}'

# 오래된 메모리 수정 + 인덱스 갱신
curl -s http://127.0.0.1:3111/update \
  -H 'content-type: application/json' \
  -d '{"id":"manual_...","text":"배포는 이제 8320 포트 사용","importance":"decision"}'

# 프롬프트용 컨텍스트 팩: 노트 + 사실 + 검색 메모리
curl -s http://127.0.0.1:3111/context \
  -H 'content-type: application/json' \
  -d '{"query":"배포 포트","project":"acme"}'

# 코어 노트 블록
curl -s http://127.0.0.1:3111/notes \
  -H 'content-type: application/json' \
  -d '{"key":"persona","value":"로컬 우선 코딩 메모리 어시스턴트"}'

# 저장소 통계
curl -s http://127.0.0.1:3111/stats
```

저널로 에이전트가 학습한 내용을 검토·되돌리기:

```bash
pjournal sync --push                  # DB 내보내기 -> 커밋 -> 푸시
git -C ~/memory-journal log --oneline
git -C ~/memory-journal revert <문제-커밋>
```

## 아키텍처

```
   Claude Code · Claude Desktop · Cursor · Cline · Codex CLI · pi · curl …
                 |                                   |
                 | stdio MCP  (memnest --mcp)     | HTTP
                 v                                   v
        +-------------------------------------------------+
        |   core (Rust)            127.0.0.1:3111         |
        |   검색 · 사실 · 비밀 · 대시보드                  |
        +-----------------------+-------------------------+
                                |  ~/.memnest/memory.db
                                v
                       journal (npm) -> git 마크다운 미러
```

## 빌드 및 테스트

| 레이어 | 명령어 |
| ----- | -------- |
| core | `cd core && cargo build --release && cargo test` |
| pi-extension | `cd pi-extension && npm install && npm run build && npm run smoke` |
| journal | `cd journal && npm install && npm run smoke` |

각 하위 폴더는 자체 `README`·`LICENSE`·`CHANGELOG`를 가지며, 두 npm 패키지는 각 폴더에서
독립적으로 배포됩니다.

## 보안

- 기본 바인딩은 `127.0.0.1`입니다. `MEMNEST_TOKEN`이 설정되지 않으면 외부 바인딩을 거부하며, 설정 시 `Bearer` 토큰이 필요합니다.
- 비밀값은 AES-256-GCM으로 암호화되며, 마스터 키는 로컬 디스크를 벗어나지 않고 절대 내보내지지 않습니다.
- 위협 모델은 [`core/docs/SECURITY.md`](./core/docs/SECURITY.md)를 참고하세요.

## 라이선스

MIT © Blue-B
