---
name: Bug report
about: Something does not behave the way it is documented
title: ''
labels: bug
---

<!--
Before you paste anything: this issue is public and permanent, and search
engines index it. Never paste `master.key` or its contents, `MEMNEST_TOKEN` or
any other token, a secret you stored with `secret_set` or read with
`secret_get`, or the contents of `<data-dir>/archive/`. Replace personal paths
like /home/yourname/work/client-a with /home/you/project, and strip memory and
transcript text you would not publish. Redacted output is more useful than a
report you have to delete afterwards.

If the bug is that memnest exposed one of those things, do not open an issue at
all. Report it privately: see SECURITY.md.
-->

## What happened

<!-- The actual behaviour, including any error text. -->

## What you expected

<!-- What you thought would happen instead. -->

## Steps to reproduce

1.
2.
3.

## Environment

```text
memnest --version:
memnest status:
```

OS and version:

How you reach memnest (pi, Claude Code, another MCP client over stdio or HTTP,
plain HTTP calls, `memnest hook`, `memnest watch`):

## Anything else

<!-- Logs help. Run the service with RUST_LOG=info to get more of them. Read
them before pasting: logs contain file paths, project names, and query text.
Remove anything you would rather not publish. -->
