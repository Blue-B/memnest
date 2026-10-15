# Connecting ChatGPT and Grok to Memnest

<!-- markdownlint-disable MD013 -->

Memnest serves a remote MCP endpoint already. This guide covers what is missing to reach it from ChatGPT on the web and from Grok on the web, and what to check before you expose anything.

Short version: the MCP endpoint exists and works, but both services call it from their own infrastructure. That needs a public HTTPS URL, and ChatGPT's custom MCP dialog cannot send a static bearer header. A small read-only gateway covers both gaps.

## What the service already serves

| Behaviour | Value | How it was checked |
| --- | --- | --- |
| Endpoint | `POST /mcp` on the same host and port as the HTTP API, `127.0.0.1:3111` by default | Request against a running 0.4.0 service |
| Transport | Streamable HTTP, JSON responses (`content-type: application/json`) | Response headers |
| Protocol version | `2025-06-18` accepted | `initialize` |
| Server-sent events | Not offered; `GET /mcp` returns 405 | `GET /mcp` |
| Notifications | `notifications/initialized` returns 202 | Request |
| Tools | `memory_remember`, `memory_search`, `memory_get`, `memory_update`, `memory_delete` | `tools/list` |
| Secret tools | Not advertised. Added only with `MEMNEST_EXPOSE_SECRET_TOOLS=1` | `tools/list` with and without the variable, and `core/src/server/mcp.rs` |
| Authentication | `Authorization: Bearer <MEMNEST_TOKEN>` when `MEMNEST_TOKEN` is set; unauthenticated otherwise | `core/src/server/mod.rs` |

Named principals with per-project read and write grants are a separate, optional layer. See [team access](team-access.md). A principal without a write grant is refused writes over both HTTP and MCP.

## What ChatGPT and Grok require

- A URL that their servers can reach over the public internet. `127.0.0.1`, `10.x.x.x` and `192.168.x.x` are rejected, so a tunnel is required. Grok documents the tunnel step in [Custom MCP server tunneling](https://docs.x.ai/grok/connectors/custom-mcp-tunneling); the same idea applies to ChatGPT.
- A remote MCP client. Both connect outbound and treat the server as untrusted input.
- ChatGPT, on the web, offers these authentication choices for a custom MCP server: OAuth, no authentication, or OAuth-or-none. There is no field for a static `Authorization` header. Grok asks for authentication during connector setup; whether its dialog accepts a static header is not confirmed here.

So there are two things to decide, and they are separate: how the URL becomes reachable, and who is allowed to use it.

## Option A: read-only gateway (recommended)

`scripts/mcp-readonly-gateway.mjs` sits in front of the MCP endpoint. It requires a secret path segment, adds the bearer token when one is configured, advertises only the tools you allow, and refuses `tools/call` for everything else. Nothing is stored in the gateway.

```bash
# 1. Pick a secret and start the gateway on loopback.
export MEMNEST_GATEWAY_SECRET="$(head -c 24 /dev/urandom | base64 | tr -d '=+/')"
export MEMNEST_GATEWAY_TOOLS="memory_search,memory_get"
export MEMNEST_URL="http://127.0.0.1:3111"
node scripts/mcp-readonly-gateway.mjs
# read-only MCP gateway on 127.0.0.1:3113 -> http://127.0.0.1:3111/mcp, tools: memory_search, memory_get, bearer: none

# 2. Expose it. Pick one.
tailscale funnel --bg --set-path /mn http://127.0.0.1:3113
cloudflared tunnel --url http://localhost:3113
ngrok http 3113
```

Your connector URL is the public host plus `/<secret>/mcp`, for example `https://<host>/mn/<secret>/mcp`. The gateway accepts the secret in any path segment as long as the last one is `mcp`, so a path prefix from the tunnel is fine.

Environment variables:

| Variable | Default | Meaning |
| --- | --- | --- |
| `MEMNEST_GATEWAY_SECRET` | none, required | Secret path segment. Refuses to start below 24 characters |
| `MEMNEST_GATEWAY_PORT` | `3113` | Loopback port to listen on |
| `MEMNEST_GATEWAY_TOOLS` | `memory_search,memory_get` | Comma-separated tool allowlist |
| `MEMNEST_URL` | `http://127.0.0.1:3111` | Base URL of the Memnest HTTP API |
| `MEMNEST_TOKEN` | unset | Sent as a bearer token upstream when set |

With `MEMNEST_GATEWAY_TOOLS=memory_search,memory_get`, a remote model can read memories and cannot write, update or delete them, and the write tools do not appear in `tools/list` at all.

## Option B: bearer token directly

If your client can send a static `Authorization: Bearer` header, skip the gateway:

1. Set `MEMNEST_TOKEN` in the service environment and restart.
2. Every local client needs the same variable, or it receives 401. This includes the pi extension and any configured Claude Code, Codex or Cursor client.
3. Expose the port through a tunnel. The connector URL is the public host plus `/mcp`.

For a read-only connection in this shape, use the access policy from [team access](team-access.md) and give the principal read grants only.

Do not select "no authentication" in a connector while the endpoint is reachable without the gateway. Anything that learns the URL can then read the whole store.

## ChatGPT

1. Open `https://chatgpt.com/plugins` on the web.
2. Select the plus button, then **Add custom MCP server**.
3. Enter a name and description, then paste the connector URL under **Server URL**.
4. Set **Authentication** to **No authentication** when the gateway holds the secret, or configure OAuth if you run an OAuth server.
5. Acknowledge the risk notice and create the plugin.
6. Install it, then start a chat and type `@` to select it.

Custom MCP servers can be restricted by workspace permissions and security settings. If the entry is missing, check your workspace policy.

## Grok

1. Open `https://grok.com/connectors`.
2. Select **New Connector**, then **Custom**.
3. Enter the connector URL and complete whatever authentication the dialog asks for.
4. Grok discovers the exposed tools and offers them in conversations.

## Verification

Check the endpoint before blaming the client. Against a running service:

```bash
# The service answers MCP on loopback.
curl -s -X POST http://127.0.0.1:3111/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'

# Through the gateway: the allowlist is applied.
curl -s -X POST "http://127.0.0.1:3113/<secret>/mcp" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'

# Through the gateway and the tunnel, exactly what the connector sees.
curl -s -X POST "https://<host>/<secret>/mcp" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

Then send a question whose answer is only in your memories, and confirm the tool calls in the gateway log. A model that answers without calling a tool has not used Memnest, even when the answer looks right.

## Limits and security

- **Retrieved text leaves the machine.** A cloud provider receives whatever `memory_search` returned and whatever `memory_get` read. Memories contain pasted conversation text, paths and configuration values.
- **The connector URL is a credential.** Anyone who obtains it can read every memory the exposed tools can reach. Treat it like a password, and rotate it by changing the secret.
- **Read-only is not harmless.** `memory_search` can still return credentials that were pasted into a conversation earlier. Redaction covers known patterns, not every secret.
- **A remote model can be manipulated.** A retrieved memory can contain instructions aimed at the model reading it. Prompt injection is a real risk on this path. Consider exposing a narrow project instead of the whole store.
- **Availability is your machine.** Tool calls fail whenever the service, the gateway or the tunnel is stopped.
- **The gateway is not covered by the repository test suite.** It was exercised against a running 0.4.0 service on 2026-10-16: allowlist filtering, refusal of a write tool, 404 on a wrong secret, batch rejection, and a live `memory_search`. Treat that as one manual check, not as continuous verification.
- **Client-side confirmation.** ChatGPT completed the handshake and a `memory_search` plus `memory_get` round trip through the gateway on 2026-10-16. Grok was not exercised; the account used was not signed in.

## Turning it off

```bash
tailscale funnel --set-path /mn off     # close the public path only
pkill -f mcp-readonly-gateway.mjs       # stop the gateway
tailscale funnel --https=443 off        # note: this also stops other Funnel paths on the host
```
