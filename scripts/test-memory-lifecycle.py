#!/usr/bin/env python3
"""Real disposable HTTP/MCP/watch lifecycle regression; no model downloads.

Build core/target/debug/memnest first. Uses only core/target/test-model-cache.
The real scheduled trash GC runs 120s after startup; total timeout is bounded.
"""

import json
import os
import secrets
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = (
    Path(sys.argv[1]).resolve()
    if len(sys.argv) > 1
    else ROOT / "core/target/debug/memnest"
)
CACHE = ROOT / "core/target/test-model-cache"
assert (CACHE / "models--intfloat--multilingual-e5-base/refs/main").is_file(), (
    "cached model required"
)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise AssertionError("disposable service must not redirect")


def local_request(base, token, path, body=None):
    target = urllib.parse.urlsplit(base)
    if not (
        target.scheme == "http" and target.hostname == "127.0.0.1"
        and target.port and not target.username and not target.password
        and not target.path and not target.query and not target.fragment
        and path.startswith("/") and not path.startswith("//")
    ):
        raise AssertionError("only disposable loopback requests allowed")
    opener = urllib.request.build_opener(
        urllib.request.ProxyHandler({}), NoRedirect()
    )
    req = urllib.request.Request(
        base + path,  # noqa: S310 - caller supplies disposable loopback base
        data=None if body is None else json.dumps(body).encode(),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {token}",
        },
    )
    try:
        with opener.open(req, timeout=60) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)


def ready(request, data):
    status, health = request("/health")
    if status != 200:
        raise AssertionError("disposable service authentication/readiness failed")
    if health.get("data_dir") != str(data):
        raise AssertionError("wrong server identity")
    return True


def run():
    with tempfile.TemporaryDirectory(prefix="memnest-lifecycle-") as temporary:
        root = Path(temporary)
        data = root / "data"
        data.mkdir()
        (data / "models").symlink_to(CACHE, target_is_directory=True)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        env = {k: v for k, v in os.environ.items() if not k.startswith("MEMNEST_")}
        token = secrets.token_hex(24)
        env.update(
            MEMNEST_TOKEN=token,
            HF_HUB_OFFLINE="1",
            MEMNEST_ARCHIVE="0",
            RUST_LOG="info",
            HTTP_PROXY="http://127.0.0.1:1",
            HTTPS_PROXY="http://127.0.0.1:1",
            ALL_PROXY="http://127.0.0.1:1",
            NO_PROXY="127.0.0.1,localhost",
        )
        child = None

        def request(path, body=None):
            return local_request(base, token, path, body)

        def mcp(name, args, error=False):
            status, response = request(
                "/mcp",
                {
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {"name": name, "arguments": args},
                },
            )
            assert status == 200, response
            result = response["result"]
            assert bool(result.get("isError")) == error, result
            return result["content"][0]["text"]

        def ids(query, project):
            status, result = request(
                "/search", {"query": query, "project": project, "n_results": 10}
            )
            assert status == 200, result
            return {row["id"] for row in result["results"]}

        def stop():
            nonlocal child
            if child is not None:
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=10)
                child = None

        with (root / "daemon.log").open("w+") as log:

            def start(rebuild=False):
                nonlocal child
                child = subprocess.Popen(
                    [
                        str(BINARY),
                        "--data-dir",
                        str(data),
                        "--host",
                        "127.0.0.1",
                        "--port",
                        str(port),
                    ],
                    env=dict(env, MEMNEST_REBUILD_INDEXES=str(int(rebuild))),
                    stdout=log,
                    stderr=log,
                )
                for _ in range(60):
                    assert child.poll() is None, "daemon exited"
                    try:
                        if ready(request, data):
                            return
                    except (OSError, TimeoutError):
                        pass
                    time.sleep(1)
                raise AssertionError("daemon startup timeout")

            captures = root / "captures"
            captures.mkdir()
            source = captures / "session.jsonl"
            source.write_text(
                "\n".join(
                    json.dumps(row)
                    for row in [
                        {
                            "type": "session_meta",
                            "payload": {
                                "id": "lifecycle-session",
                                "cwd": str(root / "workspace"),
                            },
                        },
                        {
                            "type": "event_msg",
                            "payload": {
                                "type": "user_message",
                                "message": "purgetoken captured conversation should stay deleted forever",
                            },
                        },
                    ]
                )
                + "\n"
            )

            def backfill(attempt):
                result = subprocess.run(
                    [
                        str(BINARY),
                        "--data-dir",
                        str(root / f"watch-{attempt}"),
                        "watch",
                        "--url",
                        base,
                        "--path",
                        str(captures),
                        "--once",
                        "--backfill",
                    ],
                    env=env,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                    timeout=90,
                )
                assert result.returncode == 0, result.stdout[-2000:]
                assert "stored 1 transcript chunk" in result.stdout, result.stdout[
                    -2000:
                ]

            try:
                start()
                old = json.loads(
                    mcp(
                        "memory_remember",
                        {
                            "text": "truthmarker service port is 8320",
                            "project": "alpha",
                            "memory_kind": "fact",
                        },
                    )
                )["id"]
                status, new = request(
                    "/add",
                    {
                        "text": "truthmarker service port is 9440",
                        "project": "alpha",
                        "metadata": {"memory_kind": "fact", "supersedes": old},
                    },
                )
                assert status == 201 and new["status"] == "succeeded", new
                current = new["id"]
                assert ids("truthmarker", "alpha") == {current}
                assert ids("truthmarker", "beta") == set()
                assert "8320" not in mcp(
                    "memory_search", {"query": "truthmarker", "project": "alpha"}
                )
                # Invalid corrections never hide the current row or create a replacement.
                for target, project in [
                    ("missing-id", "alpha"),
                    (current, "beta"),
                    (old, "alpha"),
                ]:
                    status, rejected = request(
                        "/add",
                        {
                            "text": "invalid correction",
                            "project": project,
                            "metadata": {"supersedes": target},
                        },
                    )
                    assert status >= 400, rejected
                    assert ids("truthmarker", "alpha") == {current}
                mcp(
                    "memory_update",
                    {
                        "id": current,
                        "text": "updatedmarker service socket is local",
                        "project": "beta",
                    },
                )
                assert ids("updatedmarker", "beta") == {current}
                assert ids("updatedmarker", "alpha") == set()
                assert ids("truthmarker", "all") == set()
                # The actual watcher supplies deterministic event IDs and retry acknowledgements.
                backfill(1)
                captured = ids("purgetoken", "all")
                assert len(captured) == 1, captured
                captured_id = captured.pop()
                mcp("memory_delete", {"id": captured_id})
                backfill(2)
                assert ids("purgetoken", "all") == set()
                status, restored = request("/restore", {"ids": [captured_id]})
                assert status == 200, restored
                assert ids("purgetoken", "all") == {captured_id}
                mcp("memory_delete", {"id": captured_id})
                # Age only this disposable fixture; production GC itself performs the deletion.
                with sqlite3.connect(data / "memory.db") as conn:
                    conn.execute(
                        "UPDATE chunks SET metadata=json_set(metadata, '$.trashed_at', '2000-01-01T00:00:00Z') WHERE id=?",
                        (captured_id,),
                    )
                deadline = time.monotonic() + 150
                while request("/chunk/" + captured_id)[0] != 404:
                    assert time.monotonic() < deadline, "scheduled trash GC timeout"
                    time.sleep(1)
                with sqlite3.connect(data / "memory.db") as conn:
                    assert conn.execute(
                        "SELECT id FROM transcript_tombstones"
                    ).fetchall() == [(captured_id,)]
                backfill(3)
                assert request("/chunk/" + captured_id)[0] == 404
                assert ids("purgetoken", "all") == set()
                for attempt, rebuild in [(4, False), (5, True)]:
                    stop()
                    start(rebuild)
                    backfill(attempt)
                    assert request("/chunk/" + captured_id)[0] == 404
                    assert ids("purgetoken", "all") == set()
                    assert ids("updatedmarker", "beta") == {current}
                    assert ids("updatedmarker", "alpha") == set()
                    assert ids("truthmarker", "all") == set()
                    assert "purgetoken captured" not in mcp(
                        "memory_search", {"query": "purgetoken", "project": "all"}
                    )
                print(
                    "PASS HTTP/MCP/watch: correction visibility and invalid target atomicity; update text/project indexes; "
                    "soft-delete/restore; real trash GC; backfill suppression after purge, restart and forced rebuild."
                )
            except Exception:
                log.flush()
                log.seek(0)
                print(log.read()[-5000:], file=sys.stderr)
                raise
            finally:
                stop()


if __name__ == "__main__":
    run()
