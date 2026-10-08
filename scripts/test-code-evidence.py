#!/usr/bin/env python3
"""Run real CLI/HTTP/MCP code-evidence paths on a disposable offline service.
Usage: python3 scripts/test-code-evidence.py [path/to/test-binary]
No production DB, installed binary, or agent configuration is changed.
"""

import copy
import json
import os
import runpy
import secrets
import socket
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
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise AssertionError("test endpoint returned invalid JSON") from error


def run():
    with tempfile.TemporaryDirectory(prefix="memnest-code-evidence-") as tmp:
        root = Path(tmp)
        data, workspace, other = root / "data", root / "repo", root / "other"
        for directory in (data, workspace, other):
            directory.mkdir()
        (data / "models").symlink_to(helpers["CACHE"], target_is_directory=True)
        source = workspace / "fix.rs"
        source.write_text("private-fixture-code-v1\n")
        (other / "fix.rs").write_text("private-fixture-code-v1\n")
        subprocess.run(["git", "init", "-q", str(workspace)], check=True)

        def commit():
            subprocess.run(["git", "-C", str(workspace), "add", "."], check=True)
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(workspace),
                    "-c",
                    "user.name=Memnest fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "commit",
                    "-qm",
                    "fixture",
                ],
                check=True,
            )

        commit()
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base, token = f"http://127.0.0.1:{port}", secrets.token_hex(24)
        env = {k: v for k, v in os.environ.items() if not k.startswith("MEMNEST_")}
        env.update(
            MEMNEST_TOKEN=token,
            HF_HUB_OFFLINE="1",
            MEMNEST_ARCHIVE="0",
            HTTP_PROXY="http://127.0.0.1:1",
            HTTPS_PROXY="http://127.0.0.1:1",
            ALL_PROXY="http://127.0.0.1:1",
            NO_PROXY="127.0.0.1,localhost",
        )

        def request(path, body=None):
            opener = urllib.request.build_opener(
                urllib.request.ProxyHandler({}), helpers["NoRedirect"]()
            )
            req = urllib.request.Request(  # noqa: S310 - isolated loopback only
                base + path,
                data=None if body is None else json.dumps(body).encode(),
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {token}",
                },
            )
            try:
                response = opener.open(req, timeout=60)  # noqa: S310 - no redirects/proxies
            except urllib.error.HTTPError as error:
                response = error
            with response:
                return response.code, decode(response.read())

        def cli(*args, cwd=workspace, ok=True, auth=token) -> Any:
            result = subprocess.run(
                [
                    str(helpers["BINARY"]),
                    "evidence",
                    "--url",
                    base,
                    "--cwd",
                    str(cwd),
                    *args,
                ],
                env=dict(env, MEMNEST_TOKEN=auth),
                capture_output=True,
                text=True,
                timeout=45,
            )
            require((result.returncode == 0) == ok, result.stdout + result.stderr)
            require(token not in result.stdout + result.stderr, "token disclosure")
            return decode(result.stdout) if ok else result.stderr

        def remember(text="evidencefixture caller reported build fix", supersedes=None):
            args = [
                "remember",
                "--text",
                text,
                "--file",
                "fix.rs",
                "--status",
                "reported_success",
                "--applicability",
                "Fixture working files; not independently verified",
                "--evidence",
                "Caller reports exit 0",
            ]
            if supersedes:
                args += ["--supersedes", supersedes]
            reply = cli(*args)
            require(reply["status"] == "succeeded", reply)
            return reply["id"]

        def get(id):
            code, row = request("/chunk/" + id)
            require(code == 200, row)
            return row

        def mcp(name, arguments):
            code, reply = request(
                "/mcp",
                {
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {"name": name, "arguments": arguments},
                },
            )
            require(code == 200, reply)
            return reply["result"]

        with (root / "daemon.log").open("w+") as log:
            child = subprocess.Popen(
                [
                    str(helpers["BINARY"]),
                    "--data-dir",
                    str(data),
                    "--host",
                    "127.0.0.1",
                    "--port",
                    str(port),
                ],
                env=env,
                stdout=log,
                stderr=log,
            )
            try:
                for _ in range(60):
                    require(child.poll() is None, "test service exited")
                    try:
                        if helpers["ready"](request, data):
                            break
                    except (OSError, TimeoutError):
                        pass
                    time.sleep(1)
                else:
                    raise AssertionError("service startup timeout")

                require(
                    request("/health")[1]["capabilities"].get("code_evidence"),
                    "capability missing",
                )
                id = remember()
                row = get(id)
                baseline = row["code_evidence"]
                require(
                    baseline["assertion"]
                    == "caller_recorded_fingerprints_not_solution_verification",
                    baseline,
                )
                require(len(baseline["git_head"]) in (40, 64), "Git HEAD not captured")
                require(
                    "private-fixture-code" not in json.dumps(row),
                    "file body was exposed",
                )
                require(cli("check", id)["status"] == "unchanged", "first check")
                require(
                    cli("check", id, cwd=other)["status"] == "different_workspace",
                    "scope mismatch",
                )
                (workspace / "unrelated.txt").write_text("unrelated commit")
                commit()
                report = cli("check", id)
                require(report["status"] == "unchanged", report)
                require(
                    report["current_git_head"] != report["saved_git_head"],
                    "unrelated commit fixture",
                )
                source.write_text("private-fixture-code-v2\n")
                report = cli("check", id)
                require(
                    report["status"] == "changed"
                    and report["files"][0]["status"] == "changed",
                    report,
                )
                require(
                    get(id)["code_evidence"] == baseline,
                    "checking rewrote saved baseline",
                )

                # Same text with new evidence is not deduplicated into an old snapshot.
                second = remember()
                require(
                    second != id and cli("check", second)["status"] == "unchanged",
                    "baseline dedup",
                )
                replacement = remember(
                    "evidencefixture revised report", supersedes=second
                )
                require(
                    get(replacement)["provenance"]["supersedes"] == second,
                    "supersedes missing",
                )
                require(get(second)["code_evidence"] is not None, "old evidence lost")

                # Do not let ordinary edits rewrite a report's basis in place.
                for body in (
                    {"id": id, "text": "replacement text"},
                    {
                        "id": id,
                        "metadata": {
                            "code_evidence": {
                                k: v for k, v in baseline.items() if k != "assertion"
                            }
                        },
                    },
                ):
                    require(
                        request("/update", body)[0] == 409, "evidence rewrite accepted"
                    )
                require(
                    get(id)["document"] == row["document"],
                    "rejected update changed text",
                )
                require(
                    request("/update", {"id": id, "importance": "decision"})[0] == 200,
                    "ordinary metadata update",
                )
                require(
                    get(id)["code_evidence"] == baseline,
                    "ordinary update erased baseline",
                )

                # Legacy rows are explicitly not recorded, not implicitly current.
                code, plain = request(
                    "/add",
                    {"text": "evidencefixture no baseline", "cwd": str(workspace)},
                )
                require(code == 201, plain)
                require(
                    cli("check", plain["id"])["status"] == "not_recorded",
                    "legacy evidence status",
                )
                require(
                    decode(mcp("memory_get", {"id": id})["content"][0]["text"])[
                        "code_evidence"
                    ]
                    == baseline,
                    "MCP get differs",
                )
                raw = {k: v for k, v in baseline.items() if k != "assertion"}
                reply = mcp(
                    "memory_remember",
                    {
                        "text": "evidencefixture MCP baseline",
                        "cwd": str(workspace),
                        "code_evidence": raw,
                    },
                )
                require(not reply.get("isError"), reply)
                mcp_id = decode(reply["content"][0]["text"])["id"]
                require(
                    get(mcp_id)["code_evidence"] == baseline,
                    "MCP remember lost baseline",
                )
                # A snapshot-only record must also reject changes to its document.
                require(
                    request("/update", {"id": mcp_id, "text": "rewritten"})[0] == 409,
                    "snapshot-only rewrite accepted",
                )

                for path in ("../outside", "/absolute", ".git/config"):
                    bad = copy.deepcopy(raw)
                    bad["files"][0]["path"] = path
                    require(
                        request(
                            "/add",
                            {"text": "invalid", "metadata": {"code_evidence": bad}},
                        )[0]
                        == 400,
                        "unsafe baseline accepted",
                    )
                    require(
                        mcp(
                            "memory_remember", {"text": "invalid", "code_evidence": bad}
                        ).get("isError"),
                        "MCP unsafe baseline accepted",
                    )
                require(
                    cli("check", id, ok=False, auth="wrong-token").find("401") >= 0,
                    "auth not enforced",
                )
                cli(
                    "remember",
                    "--text",
                    "invalid",
                    "--file",
                    "../outside",
                    "--status",
                    "failed",
                    "--applicability",
                    "fixture",
                    ok=False,
                )
                source.unlink()
                require(
                    cli("check", id)["files"][0]["status"] == "missing",
                    "missing file not identified",
                )
                source.symlink_to(other / "fix.rs")
                require(
                    cli("check", id)["files"][0]["status"] == "unavailable",
                    "symlink was read",
                )

                code, result = request(
                    "/search",
                    {
                        "query": "evidencefixture",
                        "cwd": str(workspace),
                        "n_results": 50,
                    },
                )
                require(
                    code == 200
                    and replacement in {item["id"] for item in result["results"]},
                    "saved report not searchable",
                )
                require(
                    second not in {item["id"] for item in result["results"]},
                    "superseded report visible",
                )
                require(
                    get(id)["code_evidence"] == baseline, "last check mutated baseline"
                )
                print(
                    "PASS: CLI save/check, selected-file changes, unrelated commit, workspace boundary, legacy/MCP, immutable evidence, supersedes, authentication, traversal, missing file and symlink"
                )
            finally:
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=10)


if __name__ == "__main__":
    run()
