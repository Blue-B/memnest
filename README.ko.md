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

AI에게 평소 말투로 물어보면 됩니다.

> 작업 관리 도구를 바꾼 이유가 뭐였지? 그때 논의를 찾아줘.

AI가 검색하고 원문 기록을 읽은 뒤, 확인할 수 있는 출처 ID와 함께 답합니다. 검색 범위는 현재 작업공간과 공통 `playbook`이고, 전체를 찾으려면 `project=all`을 요청합니다.

도구: `memory_search`, `memory_get`, `memory_remember`, `memory_update`, `memory_delete`, `task_card`

[![한 클라이언트가 저장한 결정을 다른 클라이언트가 같은 기록으로 찾아온 결과](docs/demo-result.ko.png)](docs/demo.ko.md)

## 작업 카드

며칠 걸리는 작업에 씁니다. 카드 본문은 AI가 쓰고, 사용자 말은 Memnest가 원문 그대로 붙이며, 다음 세션은 이 카드를 열어 이어 갑니다. 5일 동안 이어 쓴 실제 pi 세션을 다시 돌려 보니, 답 한 번에 읽은 맥락이 321K 토큰에서 174K 토큰으로 줄었습니다. 블라인드 채점에서 사용자 방침을 지킨 경우는 카드가 있을 때 3번 중 3번, 없을 때 3번 중 0번이었습니다. 카드가 있다고 모델이 반드시 따르는 것은 아닙니다. [동작 방식과 한계](docs/task-cards.ko.md)

## 그 밖의 기능

대화 수집(`memnest watch`), [과거 기록 가져오기](docs/history-import.ko.md), [코드 근거](docs/code-evidence.ko.md), 자격 증명용 암호화 금고, [팀 권한](docs/team-access.ko.md), ChatGPT와 Claude, Grok용 [웹 커넥터](docs/web-connectors.ko.md)가 있습니다. 자동 회상은 직접 켜야 동작합니다.

## 알아둘 점

- 검색이 기록을 놓칠 수 있고, 옛 결정이 지금도 맞는지는 판별하지 못합니다. 언어 모델 없이 잰 수치로, LoCoMo에서는 질문의 69%에서 근거가 처음 10개 결과 안에 있고, LongMemEval-S 표본 100개에서는 94%에서 맞는 세션이 처음 10개 안에 있습니다([검색 벤치마크](docs/benchmarks.ko.md), 답변 정확도 점수가 아닙니다).
- 보이는 대화 텍스트만 수집합니다. 추론 내용, 도구 출력, 이미지는 가져오지 않습니다.
- AI가 읽은 기억은 그 AI 제공자에게 전달됩니다. 일반 기억은 암호화되지 않으니 3111 포트를 인터넷에 열지 마세요. 웹 커넥터 URL은 비밀번호처럼 다뤄야 합니다.
- 삭제한 기록은 휴지통에 30일 남고, 백업에는 복사본이 남을 수 있습니다. [보안과 삭제 범위](SECURITY.ko.md)

## 문서

[운영, 업그레이드, 백업](docs/operations.ko.md), [변경 이력](core/CHANGELOG.ko.md), [개발 안내](CONTRIBUTING.ko.md)

## 라이선스

MIT © Blue-B
