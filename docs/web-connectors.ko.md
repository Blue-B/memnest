# ChatGPT, Claude, Grok을 Memnest에 연결하기

<!-- markdownlint-disable MD013 -->

[English](web-connectors.md) | [한국어](web-connectors.ko.md)

Memnest는 이미 원격 MCP 주소를 제공합니다. 이 문서는 클라우드에서 돌아가는 AI(ChatGPT, Claude 커스텀 커넥터, Grok)가 그 주소에 닿으려면 무엇이 더 필요한지, 그리고 외부에 열기 전에 무엇을 확인해야 하는지 다룹니다. Claude Code, Codex, Cursor, pi는 내 컴퓨터에서 돌아가며 `127.0.0.1`로 바로 연결되므로 이 문서가 필요 없습니다.

Memnest가 무엇이고 로컬 에이전트를 어떻게 연결하는지는 [README](../README.ko.md)부터 보세요.

이 문서에서 쓰는 게이트웨이는 [`scripts/mcp-readonly-gateway.mjs`](../scripts/mcp-readonly-gateway.mjs)입니다.

요약: MCP 주소는 있고 동작합니다. 다만 이 서비스들은 자기 서버에서 접속합니다. 그래서 공개 HTTPS 주소가 필요하고, ChatGPT의 커스텀 MCP 화면은 고정 bearer 헤더를 보낼 수 없습니다. 작은 읽기 전용 게이트웨이가 두 문제를 해결합니다.

## 서비스가 이미 제공하는 것

| 동작 | 값 | 확인 방법 |
| --- | --- | --- |
| 주소 | HTTP API와 같은 호스트와 포트의 `POST /mcp`. 기본값 `127.0.0.1:3111` | 실행 중인 0.4.0 서비스에 요청 |
| 통신 방식 | Streamable HTTP, JSON 응답(`content-type: application/json`) | 응답 헤더 |
| 프로토콜 버전 | `2025-06-18` 허용 | `initialize` |
| 서버 전송 이벤트(SSE) | 제공하지 않음. `GET /mcp`는 405 | `GET /mcp` |
| 알림 | `notifications/initialized`는 202 | 요청 |
| 도구 | `memory_remember`, `memory_search`, `memory_get`, `memory_update`, `memory_delete`, `task_card` | `tools/list` |
| 비밀 도구 | 기본으로 알리지 않음. `MEMNEST_EXPOSE_SECRET_TOOLS=1`일 때만 추가 | 변수 유무에 따른 `tools/list`, `core/src/server/mcp.rs` |
| 인증 | `MEMNEST_TOKEN`이 있으면 `Authorization: Bearer <MEMNEST_TOKEN>`, 없으면 인증 없음 | `core/src/server/mod.rs` |

프로젝트별 읽기/쓰기 권한이 있는 이름 있는 사용자는 별도의 선택 기능입니다. [팀 권한](team-access.ko.md)을 보세요. 쓰기 권한이 없는 사용자는 HTTP와 MCP 모두에서 쓰기가 거부됩니다.

## 클라우드 AI가 요구하는 것

- 그들의 서버가 공개 인터넷으로 닿을 수 있는 주소. `127.0.0.1`, `10.x.x.x`, `192.168.x.x`는 거부되므로 터널이 필요합니다. Grok은 [Custom MCP server tunneling](https://docs.x.ai/grok/connectors/custom-mcp-tunneling)에 터널 단계를 설명하고, 같은 방법이 ChatGPT에도 적용됩니다.
- 원격 MCP 클라이언트. 모두 바깥으로 접속하며, 서버를 믿을 수 없는 입력으로 다룹니다.
- 웹의 ChatGPT는 커스텀 MCP 서버 인증으로 OAuth, 인증 없음, OAuth 또는 없음 중에서 고르게 합니다. 고정 `Authorization` 헤더를 넣는 칸은 없습니다. Claude는 OAuth나 "로그인 없음(No sign in)"에 더해, `authorization` 같은 고정 요청 헤더를 넣을 수 있습니다. 다만 Anthropic은 요청 헤더를 일부 조직에만 열린 베타라고 설명합니다. Grok은 커넥터 설정 때 인증을 묻는데, 고정 헤더를 받는지는 여기서 확인하지 않았습니다.

그래서 정할 것이 두 가지이고, 서로 별개입니다. 주소를 어떻게 외부에서 닿게 할지, 그리고 누가 쓸 수 있게 할지입니다.

## 방법 A: 읽기 전용 게이트웨이 (권장)

[`scripts/mcp-readonly-gateway.mjs`](../scripts/mcp-readonly-gateway.mjs) 게이트웨이는 MCP 주소 앞에 섭니다. 비밀 경로 조각을 요구하고, 토큰이 설정돼 있으면 bearer를 붙이고, 허용한 도구만 알리고, 나머지 `tools/call`은 거부합니다. 게이트웨이는 아무것도 저장하지 않습니다.

```bash
# 1. 비밀값을 정하고 루프백에서 게이트웨이를 시작합니다.
export MEMNEST_GATEWAY_SECRET="$(head -c 24 /dev/urandom | base64 | tr -d '=+/')"
export MEMNEST_GATEWAY_TOOLS="memory_search,memory_get"
export MEMNEST_URL="http://127.0.0.1:3111"
node scripts/mcp-readonly-gateway.mjs
# read-only MCP gateway on 127.0.0.1:3113 -> http://127.0.0.1:3111/mcp, tools: memory_search, memory_get, bearer: none

# 2. 외부에 엽니다. 하나를 고르세요.
tailscale funnel --bg --set-path /mn http://127.0.0.1:3113
cloudflared tunnel --url http://localhost:3113
ngrok http 3113
```

커넥터 주소는 공개 호스트에 `/<secret>/mcp`를 붙인 것입니다. 예: `https://<host>/mn/<secret>/mcp`. 게이트웨이는 마지막 조각이 `mcp`이면 어느 경로 조각에 비밀값이 있어도 받아들이므로, 터널이 붙이는 경로 앞부분이 있어도 괜찮습니다.

환경 변수:

| 변수 | 기본값 | 의미 |
| --- | --- | --- |
| `MEMNEST_GATEWAY_SECRET` | 없음, 필수 | 비밀 경로 조각. 24자보다 짧으면 시작하지 않습니다 |
| `MEMNEST_GATEWAY_PORT` | `3113` | 받을 루프백 포트 |
| `MEMNEST_GATEWAY_TOOLS` | `memory_search,memory_get` | 쉼표로 구분한 허용 도구 목록 |
| `MEMNEST_URL` | `http://127.0.0.1:3111` | Memnest HTTP API 기본 주소 |
| `MEMNEST_TOKEN` | 없음 | 설정하면 뒤쪽 서비스에 bearer 토큰으로 보냅니다 |

`MEMNEST_GATEWAY_TOOLS=memory_search,memory_get`이면 원격 모델은 기억을 읽을 수만 있고 쓰거나 고치거나 지울 수 없으며, 쓰기 도구는 `tools/list`에 아예 나오지 않습니다.

## 방법 B: bearer 토큰 직접 쓰기

클라이언트가 고정 `Authorization: Bearer` 헤더를 보낼 수 있다면 게이트웨이를 건너뛸 수 있습니다.

1. 서비스 환경에 `MEMNEST_TOKEN`을 설정하고 다시 시작합니다.
2. 모든 로컬 클라이언트도 같은 변수가 필요하며, 없으면 401을 받습니다. pi 확장과 설정한 Claude Code, Codex, Cursor 클라이언트가 모두 해당됩니다.
3. 터널로 포트를 엽니다. 커넥터 주소는 공개 호스트에 `/mcp`를 붙인 것입니다.

이 방식으로 읽기 전용 연결을 하려면 [팀 권한](team-access.ko.md)의 접근 정책을 쓰고, 그 사용자에게 읽기 권한만 주세요.

게이트웨이 없이 주소가 외부에서 닿는 상태에서는 커넥터에 "인증 없음"을 고르지 마세요. 주소를 아는 누구나 저장소 전체를 읽을 수 있게 됩니다.

## ChatGPT

1. 웹에서 `https://chatgpt.com/plugins`를 엽니다.
2. 더하기 버튼을 누르고 **Add custom MCP server**를 고릅니다.
3. 이름과 설명을 넣고, **Server URL**에 커넥터 주소를 붙여 넣습니다.
4. 게이트웨이가 비밀값을 갖고 있으면 **Authentication**을 **No authentication**으로, OAuth 서버를 운영한다면 OAuth로 설정합니다.
5. 위험 안내를 확인하고 플러그인을 만듭니다.
6. 설치한 뒤 대화에서 `@`를 입력해 고릅니다.

커스텀 MCP 서버는 워크스페이스 권한과 보안 설정으로 막혀 있을 수 있습니다. 항목이 안 보이면 워크스페이스 정책을 확인하세요.

## Claude

Claude 커스텀 커넥터는 Anthropic 클라우드에서 접속하며, 커넥터를 지원하는 모든 Claude 앱(claude.ai, Claude Desktop, Cowork, 모바일 앱)에서 같습니다([Anthropic 안내](https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp)).

1. **Customize > Connectors**에서 커스텀 커넥터를 추가합니다(Team과 Enterprise는 소유자가 **Organization settings > Connectors**에서 추가).
2. 이름과 커넥터 주소를 넣고 계속합니다.
3. 게이트웨이가 비밀값을 갖고 있으면 **Authentication**을 **No sign in**으로 둡니다. 조직에 요청 헤더 기능이 있다면 대신 방법 B를 쓰고 **Request headers**에 `authorization: Bearer <MEMNEST_TOKEN>`을 넣을 수 있습니다.
4. 대화에서 커넥터를 켭니다.

## Grok

1. `https://grok.com/connectors`를 엽니다.
2. **New Connector**, 그다음 **Custom**을 고릅니다.
3. 커넥터 주소를 넣고, 화면이 묻는 인증을 마칩니다.
4. Grok이 공개된 도구를 찾아 대화에서 쓸 수 있게 합니다.

## 확인

클라이언트를 탓하기 전에 주소부터 확인하세요. 실행 중인 서비스에 대고:

```bash
# 서비스가 루프백에서 MCP에 응답합니다.
curl -s -X POST http://127.0.0.1:3111/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'

# 게이트웨이를 거치면 허용 목록이 적용됩니다.
curl -s -X POST "http://127.0.0.1:3113/<secret>/mcp" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'

# 게이트웨이와 터널을 거치면, 커넥터가 보는 그대로입니다.
curl -s -X POST "https://<host>/<secret>/mcp" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

그다음 답이 내 기억에만 있는 질문을 보내고, 게이트웨이 로그에서 도구 호출을 확인하세요. 도구를 부르지 않고 답한 모델은, 답이 맞아 보여도 Memnest를 쓴 것이 아닙니다.

## 한계와 보안

- **검색된 텍스트가 컴퓨터 밖으로 나갑니다.** 클라우드 제공자는 `memory_search`가 돌려준 것과 `memory_get`이 읽은 것을 모두 받습니다. 기억에는 붙여 넣은 대화, 경로, 설정 값이 들어 있습니다.
- **커넥터 주소가 자격 증명입니다.** 주소를 얻은 사람은 공개된 도구가 닿는 모든 기억을 읽을 수 있습니다. 비밀번호처럼 다루고, 비밀값을 바꿔서 교체하세요.
- **읽기 전용이라도 안전하지 않습니다.** 예전 대화에 붙여 넣은 자격 증명을 `memory_search`가 돌려줄 수 있습니다. 가리기는 알려진 패턴만 처리하며 모든 비밀을 잡지 못합니다.
- **원격 모델이 조작될 수 있습니다.** 검색된 기억에 그것을 읽는 모델을 노린 지시가 들어 있을 수 있습니다. 이 경로에서는 프롬프트 주입이 실제 위험입니다. 저장소 전체 대신 좁은 프로젝트만 여는 것을 고려하세요.
- **가용성은 내 컴퓨터에 달려 있습니다.** 서비스, 게이트웨이, 터널 중 하나라도 멈추면 도구 호출이 실패합니다.
- **게이트웨이는 저장소 테스트에 포함되지 않습니다.** 2026-10-16에 실행 중인 0.4.0 서비스에 대고 허용 목록 필터링, 쓰기 도구 거부, 틀린 비밀값의 404, 묶음 요청 거부, 실제 `memory_search`를 확인했습니다. 한 번의 수동 확인이며 지속적인 검증이 아닙니다.
- **클라이언트 쪽 확인.** 2026-10-16에 ChatGPT가 게이트웨이를 거쳐 연결 확인과 `memory_search`, `memory_get` 왕복을 마쳤습니다. 2026-11-04에는 같은 공개 게이트웨이가 컴퓨터 밖에서 온 `initialize`와 `tools/list`에 응답했고, 이는 Claude 커넥터가 하는 연결 확인과 같습니다. Claude 계정에 실제로 추가해 보지는 않았습니다. Grok은 계정에 로그인되어 있지 않아 시험하지 않았습니다.

## 끄기

```bash
tailscale funnel --set-path /mn off     # 공개 경로만 닫기
pkill -f mcp-readonly-gateway.mjs       # 게이트웨이 멈추기
tailscale funnel --https=443 off        # 주의: 이 호스트의 다른 Funnel 경로도 모두 멈춤
```
