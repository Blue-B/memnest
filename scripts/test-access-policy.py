#!/usr/bin/env python3
"""Real HTTP/MCP authorization and audit checks in an isolated offline store."""

import hashlib
import json
import os
import runpy
import secrets
import socket
import sqlite3
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

helpers = runpy.run_path(str(Path(__file__).with_name("test-memory-lifecycle.py")))


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def decode(raw) -> Any:
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, UnicodeDecodeError):
        return None


def run():
    with tempfile.TemporaryDirectory(prefix="memnest-access-") as temporary:
        root = Path(temporary)
        data = root / "data"
        data.mkdir(mode=0o700)
        (data / "models").symlink_to(helpers["CACHE"], target_is_directory=True)
        tokens = {
            name: secrets.token_hex(24)
            for name in ["admin", "reader-a", "writer-a", "reader-b", "empty"]
        }
        policy: dict[str, Any] = {
            "principals": [
                {
                    "id": name,
                    "token_sha256": hashlib.sha256(token.encode()).hexdigest(),
                    "admin": name == "admin",
                    "read_projects": ["alpha"]
                    if name.endswith("-a")
                    else (["beta"] if name == "reader-b" else []),
                    "write_projects": ["alpha"] if name == "writer-a" else [],
                }
                for name, token in tokens.items()
            ]
        }
        policy["retention_days"] = {"alpha": 30}
        policy_path = root / "policy.json"
        policy_path.write_text(json.dumps(policy))
        policy_path.chmod(0o600)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        env = {k: v for k, v in os.environ.items() if not k.startswith("MEMNEST_")}
        env.update(
            {
                "MEMNEST_ACCESS_POLICY": str(policy_path),
                "MEMNEST_TOKEN": tokens["admin"],
                "MEMNEST_EXPOSE_SECRET_TOOLS": "1",
                "MEMNEST_ARCHIVE": "0",
                "HF_HUB_OFFLINE": "1",
                "HTTP_PROXY": "http://127.0.0.1:1",
                "HTTPS_PROXY": "http://127.0.0.1:1",
                "ALL_PROXY": "http://127.0.0.1:1",
                "NO_PROXY": "127.0.0.1,localhost",
            }
        )

        def request(path, body=None, actor="admin", method=None):
            token = tokens.get(actor, "wrong-token")
            req = urllib.request.Request(  # noqa: S310 - disposable loopback only
                base + path,
                data=None if body is None else json.dumps(body).encode(),
                method=method,
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {token}",
                },
            )
            opener = urllib.request.build_opener(
                urllib.request.ProxyHandler({}), helpers["NoRedirect"]()
            )
            try:
                response = opener.open(req, timeout=60)  # noqa: S310 - no redirects/proxies
            except urllib.error.HTTPError as error:
                response = error
            with response:
                raw = response.read()
                value = decode(raw)
                require(
                    all(t.encode() not in raw for t in tokens.values()),
                    "token disclosure",
                )
                return response.code, value

        def add(project, text, actor="admin", metadata=None):
            code, row = request(
                "/add", {"text": text, "project": project, "metadata": metadata}, actor
            )
            require(code == 201, row)
            return row["id"]

        def get(id, actor="admin", extra=""):
            return request("/chunk/" + id + extra, actor=actor)

        def mcp(name, args, actor="reader-a", error=False):
            code, row = request(
                "/mcp",
                {
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {"name": name, "arguments": args},
                },
                actor,
            )
            require(code == 200 and bool(row["result"].get("isError")) == error, row)
            return row["result"]["content"][0]["text"]

        with (root / "daemon.log").open("w+") as log:
            child = subprocess.Popen(
                [str(helpers["BINARY"]), "--data-dir", str(data), "--port", str(port)],
                env=env,
                stdout=log,
                stderr=log,
            )
            try:
                for _ in range(60):
                    require(child.poll() is None, "isolated daemon exited")
                    try:
                        if helpers["ready"](request, data):
                            break
                    except (OSError, TimeoutError):
                        pass
                    time.sleep(0.2)
                else:
                    raise AssertionError("isolated daemon startup timeout")
                require(
                    request("/health", actor="unknown")[0] == 401,
                    "invalid token accepted",
                )
                a = add("alpha", "accessmarker alpha private content")
                b = add("beta", "accessmarker beta private content")
                require(get(a, "reader-a")[0] == 200, "own record unreadable")
                require(
                    get(b, "reader-a")[0] == 404, "ID lookup bypassed project grants"
                )
                require(get(a, "reader-b")[0] == 404, "reverse ID leak")
                for actor, project, allowed in [
                    ("reader-a", "all", {a}),
                    ("reader-b", "all", {b}),
                ]:
                    code, result = request(
                        "/search",
                        {"query": "accessmarker", "project": project, "n_results": 50},
                        actor,
                    )
                    require(
                        code == 200 and {r["id"] for r in result["results"]} == allowed,
                        result,
                    )
                require(
                    request(
                        "/search",
                        {"query": "accessmarker", "project": "beta"},
                        "reader-a",
                    )[0]
                    == 403,
                    "explicit scope bypass",
                )
                require(
                    request(
                        "/search", {"query": "accessmarker", "project": "all"}, "empty"
                    )[0]
                    == 403,
                    "empty grants failed open",
                )
                code, pack = request(
                    "/context", {"query": "accessmarker", "project": "all"}, "reader-a"
                )
                require(
                    code == 200 and "beta private content" not in json.dumps(pack),
                    "context leaked beta",
                )
                require(
                    request("/health", actor="reader-a")[1]["data_dir"] == "restricted",
                    "global path disclosed",
                )
                require(
                    request("/stats", actor="reader-a")[0] == 403,
                    "global statistics disclosed",
                )
                require(
                    request("/audit", actor="reader-a")[0] == 403,
                    "audit ledger disclosed",
                )
                for body in [
                    {"text": "must not store", "project": "alpha"},
                    {"text": "must not store", "project": "beta"},
                ]:
                    require(
                        request("/add", body, "reader-a")[0] == 403,
                        "reader write accepted",
                    )
                require(
                    request(
                        "/add", {"text": "no beta write", "project": "beta"}, "writer-a"
                    )[0]
                    == 403,
                    "writer crossed scope",
                )
                forged = add(
                    "alpha",
                    "accessmarker forged scope",
                    "writer-a",
                    {
                        "scope_project": "beta",
                        "original_project": "beta",
                        "chunk_type": "manual",
                        "importance": "knowledge",
                    },
                )
                require(
                    request("/delete", {"ids": [forged]}, "writer-a")[0] == 200,
                    "own deletion refused",
                )
                require(
                    get(forged, "reader-a")[0] == 200
                    and get(forged, "reader-b")[0] == 404,
                    "forged metadata authorized hidden row",
                )
                require(
                    request("/restore", {"ids": [forged]}, "reader-a")[0] == 403,
                    "reader restore accepted",
                )
                require(
                    request("/restore", {"ids": [forged]}, "writer-a")[0] == 200,
                    "own restore refused",
                )
                require(
                    request("/delete", {"ids": [a, b]}, "writer-a")[0] == 404,
                    "cross-scope batch accepted",
                )
                require(
                    get(a)[1]["project"] == "alpha",
                    "rejected batch partially deleted alpha",
                )
                require(
                    request("/restore", {"ids": [a, b]}, "writer-a")[0] == 404,
                    "cross-scope restore batch accepted",
                )
                require(
                    request("/update", {"id": a, "project": "beta"}, "writer-a")[0]
                    == 403,
                    "project move bypassed grants",
                )
                require(
                    request("/update", {"id": a, "text": "reader edit"}, "reader-a")[0]
                    == 403,
                    "reader edit accepted",
                )
                require(
                    request(
                        "/prune", {"project": "alpha", "keep_latest": 0}, "reader-a"
                    )[0]
                    == 403,
                    "reader prune accepted",
                )
                corrected = add(
                    "alpha",
                    "accessmarker corrected alpha",
                    "writer-a",
                    {"supersedes": a},
                )
                require(
                    get(a, "reader-a")[0] == 200 and get(a, "reader-b")[0] == 404,
                    "superseded scope lost",
                )
                require(
                    request("/delete", {"ids": [a]}, "writer-a")[0] == 200,
                    "old history deletion refused",
                )
                require(
                    request("/restore", {"ids": [a]}, "writer-a")[0] == 200,
                    "history restore refused",
                )
                require(
                    get(a)[1]["project"] == "_superseded",
                    "restoring old history made stale memory active",
                )
                require(
                    get(a, "reader-a")[0] == 200, "history lost authorization scope"
                )
                require(
                    "beta private content"
                    not in mcp(
                        "memory_search", {"query": "accessmarker", "project": "all"}
                    ),
                    "MCP search leak",
                )
                require(
                    "beta private content"
                    not in mcp("memory_get", {"id": b}, error=True),
                    "MCP ID leak",
                )
                mcp(
                    "memory_remember",
                    {"text": "forbidden", "project": "alpha"},
                    error=True,
                )
                mcp("memory_update", {"id": corrected, "text": "forbidden"}, error=True)
                mcp("memory_delete", {"id": corrected}, error=True)
                require(
                    request(
                        "/secrets",
                        {"key": "fixture-secret", "value": "fixture-private-secret"},
                    )[0]
                    == 201,
                    "admin vault write",
                )
                require(
                    request("/secrets/fixture-secret", actor="reader-a")[0] == 403,
                    "HTTP vault read bypass",
                )
                require(
                    request("/secrets", actor="reader-a")[0] == 403,
                    "HTTP vault metadata bypass",
                )
                require(
                    request(
                        "/secrets/fixture-secret", actor="writer-a", method="DELETE"
                    )[0]
                    == 403,
                    "HTTP vault delete bypass",
                )
                mcp("secret_get", {"key": "fixture-secret"}, error=True)
                require(
                    request("/purge", {"ids": [corrected]}, "writer-a")[0] == 409,
                    "active purge accepted",
                )
                doomed = add("alpha", "accessmarker doomed", "writer-a")
                require(
                    request("/delete", {"ids": [doomed]}, "writer-a")[0] == 200,
                    "soft deletion failed",
                )
                require(
                    request("/purge", {"ids": [doomed]}, "reader-a")[0] == 403,
                    "reader purge accepted",
                )
                require(
                    request("/purge", {"ids": [doomed]}, "writer-a")[0] == 200,
                    "own purge refused",
                )
                require(get(doomed)[0] == 404, "purged record remained readable")
                require(
                    request("/restore", {"ids": [doomed]}, "writer-a")[1]["restored"]
                    == [],
                    "purged record restored",
                )
                transcript = {
                    "chunk_type": "auto_log",
                    "source": "fixture.transcript",
                    "event_id": "purge-replay",
                    "session_id": "fixture",
                    "role": "user",
                    "sequence": 0,
                }
                event = add(
                    "alpha", "accessmarker replay fixture", "writer-a", transcript
                )
                require(
                    request("/delete", {"ids": [event]}, "writer-a")[0] == 200,
                    "event delete",
                )
                require(
                    request("/purge", {"ids": [event]}, "writer-a")[0] == 200,
                    "event purge",
                )
                _, replay = request(
                    "/add",
                    {
                        "project": "alpha",
                        "text": "accessmarker replay fixture",
                        "metadata": transcript,
                    },
                    "writer-a",
                )
                require(
                    replay["status"] == "deduplicated" and get(event)[0] == 404,
                    "purged transcript resurrected",
                )
                expiring = add(
                    "alpha",
                    "accessmarker expiring",
                    "writer-a",
                    {"pinned": True, "importance": "knowledge"},
                )
                preserved = add("beta", "accessmarker preserved")
                with sqlite3.connect(data / "memory.db") as connection:
                    connection.execute(
                        "UPDATE chunks SET created_at=? WHERE id IN (?,?)",
                        ("2000-01-01T00:00:00Z", expiring, preserved),
                    )
                require(
                    request("/retention", {}, "reader-a")[0] == 403,
                    "reader retention run",
                )
                require(request("/retention", {})[0] == 200, "retention run failed")
                require(
                    get(expiring)[1]["project"] == "_trash",
                    "project retention did not override pinned",
                )
                require(
                    get(preserved)[1]["project"] == "beta",
                    "retention crossed unconfigured project",
                )
                require(
                    not (data / "archive").exists(),
                    "purge/retention wrote a plaintext archive",
                )
                _, audit = request("/audit")
                raw = json.dumps(audit)
                require(
                    "alpha private content" not in raw
                    and "beta private content" not in raw
                    and "fixture-private-secret" not in raw,
                    "audit retained content",
                )
                require(
                    all(t not in raw for t in tokens.values()), "audit retained token"
                )
                require(
                    any(
                        e["actor"] == "reader-a" and a in e["target_ids"]
                        for e in audit["events"]
                    ),
                    "read not attributed",
                )
                require(
                    any(
                        e["actor"] == "writer-a" and e["action"] == "memory_remember"
                        for e in audit["events"]
                    ),
                    "write not attributed",
                )
            finally:
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=10)
            # A protected store must not reopen in legacy unauthenticated mode.
            reopened = subprocess.run(
                [str(helpers["BINARY"]), "--data-dir", str(data), "--port", str(port)],
                env={k: v for k, v in env.items() if k != "MEMNEST_ACCESS_POLICY"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            require(
                reopened.returncode != 0
                and "requires MEMNEST_ACCESS_POLICY" in reopened.stderr,
                "policy removal failed open",
            )
            print(
                "PASS: real scoped HTTP/MCP reads and writes, forged scope, aliases/history, atomic permission batches, vault/stats/audit restrictions, content-free audit, protected-store reopen"
            )


if __name__ == "__main__":
    run()
