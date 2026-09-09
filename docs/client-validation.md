# Real client validation

<!-- markdownlint-disable MD013 -->

The reproducible demo uses separate MCP and HTTP clients because it can run without a model account. A second validation on 2026-09-09 launched the installed pi, Claude Code, and Codex programs against an isolated Memnest service and workspace.

## Result

| Client | Result | Evidence boundary |
| --- | --- | --- |
| pi 0.85.1 | Passed | The real pi process called `memory_search` and printed the exact seeded token followed by `PI_RECALLED`. |
| Claude Code 2.1.258 | Blocked before tool use | The account returned HTTP 403 because the organization disabled Claude subscription access for Claude Code. |
| Codex CLI 0.139.0 | Blocked before tool use | Authentication was expired, then the account reported that its usage limit had been reached. |

The machine-readable summary is [`demo-evidence/real-client-validation.json`](demo-evidence/real-client-validation.json). It contains the pi stdout and sanitized blocker messages. Account identifiers, request IDs, tokens, and local paths are omitted.

This proves one real pi recall path. It does not prove a Claude Code to Codex handoff, a Codex to Claude Code handoff, or autonomous memory use by all three clients. Those runs require working client accounts and should use the same procedure: seed or save a unique marker, start a fresh client process in the matching workspace, require a `memory_search` tool call, and compare the returned text rather than trusting the model's answer alone.
