# memnest

<!-- markdownlint-disable MD013 MD033 -->

<img src="docs/logo.png" alt="memnest 로고" width="220">

[English](README.md) | [설치](#설치) | [사용법](#사용법) | [운영 문서](docs/operations.md)

코딩 AI와 전에 무엇을 했고, 왜 그렇게 결정했는지 다시 찾는 로컬 메모리입니다.

Memnest는 pi, Claude Code, Codex의 대화 텍스트를 내 컴퓨터에 저장합니다. 연결한 AI는 세션이 바뀌어도 그 기록을 검색하고 원문을 읽을 수 있습니다. 여러 클라이언트에서 쓰려면 같은 Memnest 서비스에 연결하면 됩니다.

[![최신 릴리스](https://img.shields.io/github/v/release/Blue-B/memnest?label=release)](https://github.com/Blue-B/memnest/releases/latest)
[![npm: pi-memnest](https://img.shields.io/npm/v/pi-memnest?label=npm%20pi-memnest&color=cb3837)](https://www.npmjs.com/package/pi-memnest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

## 실제 기억 조회

[![실제 질문에서 두 번의 조회를 거쳐 기존 정리 메모의 이유를 읽은 사례](docs/recall-demo.ko.png)](docs/recall-check-2026-10-06.md)

작업 관리 도구를 왜 바꿨는지 검색하고, 결과에 연결된 기존 수동 정리 메모를 읽어 이유를 확인했습니다. 한 번의 검색으로 당시 대화를 복원한 사례는 아닙니다. 이미지는 실제 요청 결과를 편집한 것으로 화면 녹화가 아니며, [질문 3개를 시험한 결과](docs/recall-check-2026-10-06.md)에는 답을 확인하지 못한 나머지 두 질문도 남겼습니다.

## 사용 장면

이런 질문을 할 때 쌓인 기록을 찾아보는 용도입니다.

> 전에 쓰던 작업 관리 도구를 왜 바꿨지? 당시 기록을 찾아줘.
>
> 이 프로젝트에서 마지막으로 정한 배포 방법과 이유를 확인해줘.
>
> 지난번 같은 오류가 났을 때 무엇을 시도했는지 찾아줘.

AI가 기억을 검색하고, 관련된 기록의 원문을 읽은 뒤 답하는 흐름입니다. 검색 결과에는 출처 ID가 있어 답변의 근거를 다시 확인할 수 있습니다. 위 문장은 사용 예시이며, 자동 실행 데모나 정답 보장을 뜻하지 않습니다.

매번 적용할 짧은 규칙은 `AGENTS.md`나 `CLAUDE.md`에 두는 편이 간단합니다. Memnest는 계속 쌓이는 대화와 결정 기록을 필요할 때 찾아보는 쪽에 맞습니다.

## 설치

Linux x86_64와 Arm64 배포 파일을 제공합니다. Rust는 필요하지 않습니다. 처음 저장하거나 검색할 때 약 1.1GB의 임베딩 모델을 내려받고, 임베딩 중에는 RAM을 약 1.9GB까지 사용할 수 있습니다.

내려받은 스크립트를 확인한 뒤 실행하세요. 기존 설치를 업그레이드한다면 먼저 `memory.db`와 `master.key`를 함께 백업하세요.

```bash
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/v0.3.1/core/scripts/install.sh \
  -o /tmp/memnest-install.sh
VERSION=v0.3.1 bash /tmp/memnest-install.sh --user
```

설치기는 서버와 대화 감시기를 시작하고, 지원되는 Claude Code, Codex, Cursor 설정에 빠진 연결을 추가합니다. 기존 설정 변경 전 백업하고 복구 명령을 안내합니다.

### pi 연결

코어를 실행한 상태에서 같은 버전의 확장을 설치합니다.

```bash
# npm에 공개된 기본 연동
pi install npm:pi-memnest@0.3.1

# npm 배포 전 0.4.0 기능은 검토한 소스에서 연결
pi install ./pi-extension
```

다른 경로의 Memnest 확장이 이미 등록돼 있다면 먼저 그 등록을 제거해 중복 로드를 피하세요. 연결 상태는 pi의 `/memnest`에서 확인합니다. [확장 설정](pi-extension/README.md)

<details>
<summary>로컬 개발 설정: 금고 도구</summary>

확장은 기본적으로 기억 도구 5개만 노출합니다. pi에서 자격 증명을 저장하고 조회하는 `secret_set`, `secret_get`, `secret_list`, `secret_delete`를 사용하려면 pi를 시작하기 전에 금고 도구를 활성화하세요.

```bash
export MEMNEST_EXPOSE_SECRET_TOOLS=1
pi
```

환경변수를 바꾼 뒤에는 pi를 다시 시작해야 합니다. `/reload`만으로는 실행 중인 프로세스 환경이 바뀌지 않습니다. `/memnest`는 금고 도구의 사용 가능 여부를 보여주며, 숨겨진 경우 이 활성화 방법을 안내합니다.

</details>

다른 MCP 클라이언트는 `http://127.0.0.1:3111/mcp`에 연결합니다. [연결 예시](adapters/README.md), [다른 환경과 소스 설치](docs/operations.md), [업그레이드와 복구](docs/operations.md#linux-service-upgrades)를 참고하세요. macOS는 소스 설치만 제공하며 실제 설치와 제거는 검증하지 못했습니다.

## 사용법

연결한 AI에 과거 기록을 찾아달라고 요청하세요. 자동 회상은 기본으로 꺼져 있으며, 필요할 때 검색을 요청하는 방식으로 시작할 수 있습니다. 자동 회상을 원하면 [확장 설정](pi-extension/README.md)을 참고하세요.

직접 도구를 호출하는 클라이언트에서는 다음 흐름을 사용합니다.

```text
memory_search(query="작업 관리 도구를 바꾼 이유")
memory_get(id="<검색 결과의 ID>")
```

도구는 저장, 검색, 원문 조회, 정정, 삭제의 다섯 가지입니다. 중요한 결정을 `memory_remember`로 명시적으로 남길 수 있습니다. 바뀐 사실은 `supersedes=<이전 ID>`로 새 기록을 저장해 이전 기록과 연결합니다.

현재 작업 경로가 전달되면 해당 작업공간과 공통 `playbook`을 검색합니다. 다른 프로젝트까지 찾으려는 경우 `project=all`을 지정합니다. 긴 원문과 주변 대화를 읽는 방법은 [원문 조회 문서](docs/bounded-retrieval.md)에 있습니다.

설치 전의 대화는 자동으로 모두 가져오지 않습니다. 필요한 원본 파일을 선택해 [과거 기록 가져오기](docs/history-import.md)로 추가하세요.

### 코드 근거와 팀 제어 (0.4.0)

해결 기록을 제안, 실패, 보고된 성공으로 구분하되 검증된 사실로 취급하지 않습니다. `memory_remember(code_files=["상대/파일경로"])`로 선택한 파일의 기준을 붙이고, 필요할 때만 `memory_get(check_code=true)`로 비교합니다. 무관한 파일을 바꿨다고 경고하지 않습니다. [코드 근거 문서](docs/code-evidence.md)

선택적으로 [팀별 프로젝트 권한, 감사, 보존 정책](docs/team-access.md)을 켤 수 있습니다. [무료 로컬 비교](docs/local-provider-comparison.md)에는 경쟁 제품보다 나았던 항목과 부족했던 항목을 함께 공개합니다.

## 알아둘 점

- **기록 검색이지 사실 판정은 아닙니다.** 필요한 기록을 놓칠 수 있고, 옛 결정이 지금도 맞는지 자동으로 판별하지 못합니다. 후속 기록 안내도 검증된 답변 관계를 뜻하지 않습니다.
- **모든 대화를 저장하지는 않습니다.** 보이는 대화 텍스트를 수집하며, 추론 내용, 도구 입출력, 이미지와 서브에이전트 대화는 제외합니다. ChatGPT나 Claude의 내장 메모리도 가져오지 않습니다.
- **저장은 로컬, 답변은 연결한 AI가 담당합니다.** 기본 저장과 검색은 생성형 LLM을 호출하지 않습니다. `evidence_only=true`를 명시하면 설정한 로컬 모델이 근거 후보를 선택하며, 그 판단은 사실 검증이 아닙니다. 클라우드 AI에 검색 결과를 전달하면 해당 제공자가 그 내용을 받습니다.
- **일반 기억은 암호화되지 않습니다.** 알려진 비밀값 형태를 가리는 처리가 모든 유출을 막지는 못합니다. 3111 포트를 인터넷에 직접 노출하지 마세요.
- **삭제 직후 완전히 사라지지는 않습니다.** 휴지통에 기본 30일 남습니다. 새 평문 보관 파일은 기본으로 만들지 않지만 기존 보관 파일, 백업과 원본 대화에는 남을 수 있습니다. [보안과 삭제 범위](SECURITY.md)를 확인하세요.

## 문서와 검증

Memnest는 Rust 서비스 하나에서 SQLite 원문을 관리하고, BM25와 로컬 다국어 임베딩으로 검색합니다. 검색 색인은 원문에서 다시 만들 수 있습니다.

- [실제 클라이언트 검증](docs/client-validation.md)과 [공유 저장소 재현 데모](docs/demo.md)
- [후속 기록 조회 검사와 한계](docs/following-capture.md), [검색 평가](benchmarks/memorybench/README.md)
- [설계 결정](docs/design-decisions.md), [운영과 백업](docs/operations.md), [개발 안내](CONTRIBUTING.md)
- [최신 릴리스](https://github.com/Blue-B/memnest/releases/latest)와 [변경 이력](core/CHANGELOG.md)

## 라이선스

MIT © Blue-B
