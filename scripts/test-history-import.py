#!/usr/bin/env python3
"""Exercise history preview/replay through the real CLI and an isolated HTTP server.

Optional --case is a private JSON file with snapshot, transcript, session_id and
queries. Never copies vaults or writes to the source DB/transcript. Artifacts are
retained in a private temporary directory. No model API or model download.
"""

import argparse
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
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPERS = runpy.run_path(str(ROOT / "scripts/test-memory-lifecycle.py"))


def run():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "core/target/debug/memnest")
    parser.add_argument("--old", type=Path)
    parser.add_argument("--case", type=Path)
    args = parser.parse_args()
    try:
        case = json.loads(args.case.read_text()) if args.case else None
    except (OSError, json.JSONDecodeError) as error:
        parser.error(f"cannot read private case: {type(error).__name__}")
    os.umask(0o077)
    root = Path(tempfile.mkdtemp(prefix="memnest-history-check-"))
    print(json.dumps({"private_workspace": str(root)}), flush=True)
    data = root / "data"
    data.mkdir()
    if case:
        # Same active-chunk/workspace allowlist as evaluate-existing-outcomes.py.
        with sqlite3.connect(Path(case["snapshot"]).as_uri() + "?mode=ro", uri=True) as src:
            src.execute("BEGIN")
            with sqlite3.connect(data / "memory.db") as dst:
                for table, select_sql, insert_sql in [
                    ("chunks", "SELECT id,project,document,embedding,metadata,created_at,updated_at FROM chunks WHERE project NOT IN ('_trash','_superseded')", "INSERT INTO chunks VALUES (?,?,?,?,?,?,?)"),
                    ("workspaces", "SELECT id,display_name,legacy_project,first_seen_at,last_seen_at FROM workspaces", "INSERT INTO workspaces VALUES (?,?,?,?,?)"),
                ]:
                    dst.execute(src.execute("SELECT sql FROM sqlite_master WHERE name=?", (table,)).fetchone()[0])
                    cursor = src.execute(select_sql)
                    while batch := cursor.fetchmany(1000):
                        dst.executemany(insert_sql, batch)
            src.rollback()
        source = Path(case["transcript"])
        session_id = case["session_id"]
        queries = case["queries"]
    else:
        source = root / "fixture.jsonl"
        session_id = "history-import-regression"
        source.write_text("\n".join(json.dumps(row) for row in [
            {"type": "session_meta", "payload": {"id": session_id, "cwd": str(root / "workspace")}},
            {"type": "event_msg", "payload": {"type": "user_message", "message": "historyrecall 이전 작업 도구를 왜 교체했지?"}},
            {"type": "event_msg", "payload": {"type": "agent_message", "message": "historyrecall 세션을 전환할 때 오류가 나서 작업 도구를 교체했습니다."}},
        ]) + "\n")
        queries = ["historyrecall 이전 작업 도구 교체 이유"]
    original_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    (data / "models").symlink_to(HELPERS["CACHE"], target_is_directory=True)
    token = secrets.token_hex(24)
    env = {key: value for key, value in os.environ.items() if not key.startswith("MEMNEST_")}
    env.update(MEMNEST_TOKEN=token, HF_HUB_OFFLINE="1", MEMNEST_ARCHIVE="0",
               MEMNEST_TTL_AUTOLOG_DAYS="0", RUST_LOG="error",
               HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1",
               ALL_PROXY="http://127.0.0.1:1", NO_PROXY="127.0.0.1,localhost")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    base = f"http://127.0.0.1:{port}"
    state = root / "watch-state.json"

    def cli(*flags, binary=None, expected=0):
        result = subprocess.run([str(binary or args.binary), "--data-dir", str(root), "watch",
                                 "--url", base, "--path", str(source), *flags],
                                env=env, capture_output=True, text=True, timeout=300)
        assert (result.returncode == 0) == (expected == 0), result.stderr[-2000:]
        return result.stdout

    def request(path, body=None):
        return HELPERS["local_request"](base, token, path, body)

    def rows():
        with sqlite3.connect((data / "memory.db").as_uri() + "?mode=ro", uri=True) as db:
            return db.execute("SELECT id,document,project FROM chunks WHERE json_extract(metadata,'$.session_id')=?", (session_id,)).fetchall()

    def search():
        results = []
        for query in queries:
            status, reply = request("/search", {"query": query, "project": "all", "n_results": 5})
            assert status == 200, reply
            results.append(reply)
        return results

    with (root / "server.log").open("w") as log:
        child = subprocess.Popen([str(args.binary), "--data-dir", str(data), "--host", "127.0.0.1", "--port", str(port)],
                                 env=env, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 180
            while True:
                assert child.poll() is None, "disposable server exited; inspect private log"
                assert time.monotonic() < deadline, "startup timed out"
                try:
                    if HELPERS["ready"](request, data):
                        break
                except (OSError, TimeoutError):
                    pass
                time.sleep(0.2)
            assert rows() == [], "the selected source must be missing before replay"
            before = search()
            # Reproduce a watcher that has already skipped the historical prefix.
            cli("--once", binary=args.old)
            saved_state = state.read_bytes()
            if args.old:
                cli("--once", "--backfill", binary=args.old)
                assert rows() == [], "old binary unexpectedly recovered the historical source"
                saved_state = state.read_bytes()
            preview = json.loads(cli("--once", "--backfill", "--dry-run"))
            assert rows() == [] and state.read_bytes() == saved_state
            assert preview["eligible_chunks"] > 0
            if not case:
                cli("--dry-run", expected=1)
            started = time.monotonic()
            imported = json.loads(cli("--once", "--backfill"))
            import_seconds = time.monotonic() - started
            captured = rows()
            assert len(captured) == preview["eligible_chunks"] == imported["new_chunks"]
            assert imported["skipped_chunks"] == 0
            assert state.read_bytes() == saved_state
            after = search()
            captured_ids = {row[0] for row in captured}
            ranks = [[index + 1 for index, item in enumerate(reply["results"]) if item["id"] in captured_ids] for reply in after]
            fetched = []
            for reply in after:
                for item in reply["results"]:
                    if item["id"] in captured_ids:
                        status, result = request("/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
                            "name": "memory_get", "arguments": {"id": item["id"], "max_chars": 8000}}})
                        assert status == 200 and not result["result"].get("isError"), result
                        fetched.append(result)
            repeated = json.loads(cli("--once", "--backfill"))
            assert repeated["new_chunks"] == 0 and repeated["skipped_chunks"] == len(captured)
            assert rows() == captured and state.read_bytes() == saved_state
            if not case:
                assert ranks == [[1, 2]], ranks
                victim = captured[0][0]
                status, result = request("/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
                    "name": "memory_delete", "arguments": {"id": victim}}})
                assert status == 200 and not result["result"].get("isError"), result
                deleted = json.loads(cli("--once", "--backfill"))
                assert deleted["new_chunks"] == 0
                assert next(row for row in rows() if row[0] == victim)[2] == "_trash"
                assert victim not in {item["id"] for item in search()[0]["results"]}
            assert state.read_bytes() == saved_state
            assert hashlib.sha256(source.read_bytes()).hexdigest() == original_hash
            (root / "evidence.json").write_text(json.dumps({"queries": queries, "preview": preview,
                "before": before, "after": after, "fetched": fetched}, ensure_ascii=False, indent=2))
            summary = {"old_reproduced": bool(args.old), "preview_chunks": preview["eligible_chunks"],
                       "new_chunks": imported["new_chunks"], "repeat_new": repeated["new_chunks"],
                       "source_ranks_top5": ranks, "first_search_original_criterion_pass": all(ranks),
                       "import_seconds": round(import_seconds, 3),
                       "watch_state_unchanged": True, "source_unchanged": True,
                       "soft_delete_checked": not bool(case)}
            (root / "result.json").write_text(json.dumps(summary, indent=2))
            print(json.dumps(summary), flush=True)
        finally:
            child.terminate()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=10)


if __name__ == "__main__":
    run()
