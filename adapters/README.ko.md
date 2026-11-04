# Memnest 연결 방식

[English](README.md) | [한국어](README.ko.md)

Memnest 코어는 특정 플랫폼에 묶이지 않습니다. 어떤 에이전트든 HTTP API나 MCP 서버(Streamable HTTP 또는 stdio)를 쓸 수 있습니다. 어댑터는 호스트의 생명주기 이벤트를 이 고정된 작업으로 옮겨 주기만 합니다.

## 지원하는 연결 방식

| 연결 방식 | 쓰임 |
| --- | --- |
| HTTP API | 여러 클라이언트가 함께 쓰는, 계속 돌아가는 로컬 서비스 |
| Streamable HTTP 기반 MCP | API와 같은 포트의 `POST /mcp`. 여러 호스트가 프로세스 하나와 저장소 하나를 같이 씁니다. |
| stdio 기반 MCP | 자기 자식 프로세스를 띄우는 클라이언트의 도구 연결 |
| `memnest hook`, `memnest watch` | 확장 없이 어떤 호스트에든 프롬프트 시점 주입과 대화 수집을 주는 코어 명령. `SessionStart`와 `UserPromptSubmit`에서 훅은 [작업 카드](../docs/task-cards.ko.md)도 처리합니다. |
| MCP `task_card`, `POST /task-card` | MCP 클라이언트나 일반 HTTP 어댑터용 작업 카드 도구 |
| `pi-extension/` | 기본 기억 도구 5개, 직접 켜는 금고 도구 4개, 범위가 정해진 자동 회상, `/memnest` |
| `adapters/generic-http/` | 다른 호스트를 위한, 의존성 없는 JSONL 참고 어댑터 |

## 어댑터 약속

어댑터는 다음 작업을 제공해야 합니다.

- `health`: 로컬 서비스 확인
- `remember`: 지속되는 기록, 사실, 규칙, 절차 저장
- `search`: 기억 검색. 절대 경로 `cwd`, 명시적 `project`, 또는 일부러 여러 프로젝트를 찾을 때 `project=all`을 넘깁니다.

대화 수집은 일부러 이 목록에 없습니다. 모든 호스트의 대화 수집은 `memnest watch` 하나로 하므로, 어댑터가 메시지나 세션 요약까지 보내면 같은 대화가 두 번 저장됩니다. 참고 어댑터는 `message`와 `summary` 이벤트를 거부하고 `watch`를 쓰라고 안내합니다.

모든 저장 요청에는 `adapter`, `adapter_version`, `session_id`, `cwd`, `source`, `role`을 넣을 수 있습니다. `project`를 빼면 `cwd`가 고정된 개인 작업공간 범위와 `playbook`을 고릅니다. 구조화된 기억에는 `memory_kind`, `confidence`, `source_ids`, `supersedes`, `verified_at`도 넣을 수 있습니다. `supersedes`는 같은 범위의 활성 기억만 가리킬 수 있습니다. 신뢰도와 검증 시각은 출처 정보일 뿐, 서버가 사실이라고 인증한 것이 아닙니다.

어댑터 안에 에이전트 루프나 모델 클라이언트를 넣으면 안 됩니다. Claude Code, Codex, OpenCode, Cursor, 앞으로 나올 호스트가 같은 로컬 저장소를 쓸 수 있도록 작은 통신 변환기로 남아야 합니다.

## 일반 HTTP 참고 어댑터

한 줄에 JSON 객체 하나씩 보냅니다.

```bash
printf '%s\n' \
  '{"type":"health"}' \
  '{"type":"remember","cwd":"/work/demo","memory_kind":"fact","text":"Deploy uses port 8320"}' \
  '{"type":"search","cwd":"/work/demo","query":"deploy port"}' \
  | node adapters/generic-http/memnest-adapter.mjs
```

설정:

- `MEMNEST_URL`, 기본값 `http://127.0.0.1:3111`
- `MEMNEST_TOKEN`, 선택 사항인 bearer 토큰
- `MEMNEST_ADAPTER`, 기본값 `generic-http`
- `MEMNEST_TIMEOUT_MS`, 기본값 `3000`

의존성 없는 약속 테스트를 실행합니다.

```bash
node adapters/generic-http/test.mjs
```

## MCP 클라이언트

두 통신 방식은 같은 도구를 제공하며, HTTP 쪽을 권합니다. 클라이언트를 실행 중인 서비스에 연결하세요.

```json
{
  "mcpServers": {
    "memnest": { "url": "http://127.0.0.1:3111/mcp" }
  }
}
```

자식 프로세스만 띄울 수 있는 클라이언트는 stdio를 씁니다.

```json
{
  "mcpServers": {
    "memnest": {
      "command": "/absolute/path/to/memnest",
      "args": ["--mcp", "--data-dir", "/home/you/.memnest"]
    }
  }
}
```

띄운 프로세스는 실행되는 동안 데이터 폴더를 차지합니다. 두 번째 stdio 클라이언트나, HTTP 서비스와 함께 쓰는 stdio는 쓰기 잠금 때문에 거부됩니다. HTTP 방식은 모든 클라이언트가 서비스 하나와 통신하므로 이런 충돌이 없습니다.

두 형태 모두 Claude Code, Codex, OpenCode, Cursor 같은 클라이언트에 맞습니다. 호스트가 MCP 없이 생명주기 훅만 제공한다면 먼저 `memnest hook`과 `memnest watch`를 쓰세요. 구조화된 저장처럼 이 둘로 안 되는 작업이 필요할 때만 어댑터를 만듭니다.

## 기여 점검 목록

새 플랫폼 어댑터는 다음을 지켜야 합니다.

1. `adapter`와 `adapter_version`으로 자기를 밝힙니다.
2. 대화를 직접 보내지 않고 대화 수집은 `memnest watch`에 맡깁니다.
3. 오류를 기록하기 전에 자격 증명을 가립니다.
4. 요청 시간 제한을 둡니다.
5. 정상 종료 때 남은 저장 요청을 마칩니다.
6. 호스트 앱 없이 돌릴 수 있는 약속 테스트를 포함합니다.
