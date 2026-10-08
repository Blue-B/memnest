#!/usr/bin/env python3
"""Offline real HTTP/MCP approach contract. Build core/target/debug/memnest first.
Only a disposable authenticated loopback service and cached model are used.
Run: python3 scripts/test-approach-outcomes.py [path/to/test-binary]
"""

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

# Reuse the lifecycle test's loopback-only, no-proxy/no-redirect client.
helpers = runpy.run_path(str(Path(__file__).with_name("test-memory-lifecycle.py")))


def run():
    with tempfile.TemporaryDirectory(prefix="memnest-approach-") as tmp:
        data = Path(tmp) / "data"
        data.mkdir()
        (data / "models").symlink_to(helpers["CACHE"], target_is_directory=True)
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

        def request(path, body=None, auth=token) -> tuple[int, Any]:
            assert path.startswith("/") and not path.startswith("//")
            opener = urllib.request.build_opener(
                urllib.request.ProxyHandler({}), helpers["NoRedirect"]()
            )
            req = urllib.request.Request(  # noqa: S310 - fixed disposable loopback base
                base + path,
                data=None if body is None else json.dumps(body).encode(),
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {auth}",
                },
            )
            try:
                response = opener.open(req, timeout=60)  # noqa: S310 - no redirects/proxies
            except urllib.error.HTTPError as error:
                response = error
            with response:
                text = response.read().decode()
                try:
                    value = json.loads(text)
                except json.JSONDecodeError:
                    value = text
                return response.code, value

        def mcp(name, args, error=False):
            code, reply = request(
                "/mcp",
                {
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {"name": name, "arguments": args},
                },
            )
            assert code == 200, reply
            result = reply["result"]
            assert bool(result.get("isError")) == error, result
            return result["content"][0]["text"]

        def get(id):
            code, row = request("/chunk/" + id)
            assert code == 200, row
            return row

        def search(project="alpha"):
            code, reply = request(
                "/search",
                {"query": "outcomemarker", "project": project, "n_results": 50},
            )
            assert code == 200, reply
            return reply["results"]

        with (Path(tmp) / "daemon.log").open("w+") as log:
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
                    assert child.poll() is None, "test daemon exited"
                    try:
                        if helpers["ready"](request, data):
                            break
                    except (OSError, TimeoutError):
                        pass
                    time.sleep(1)
                else:
                    raise AssertionError("test daemon startup timeout")
                assert request("/health", auth="wrong-token")[0] == 401
                # Old HTTP/MCP callers retain plain-memory behavior.
                code, plain = request(
                    "/add", {"text": "outcomemarker plain", "project": "alpha"}
                )
                assert code == 201, plain
                assert get(plain["id"])["approach"] is None
                legacy = json.loads(
                    mcp(
                        "memory_remember",
                        {"text": "outcomemarker legacy", "project": "alpha"},
                    )
                )
                assert (
                    json.loads(mcp("memory_get", {"id": legacy["id"]}))["approach"]
                    is None
                )
                code, _ = request(
                    "/update",
                    {"id": legacy["id"], "text": "outcomemarker legacy revised"},
                )
                assert code == 200 and get(legacy["id"])["document"].endswith("revised")
                ids = []
                for status in ["proposed", "failed", "reported_success"]:
                    approach = {
                        "status": status,
                        "applicability": "Linux offline fixture",
                        "evidence": "",
                    }
                    # Identical text must not merge reports with different outcomes.
                    args = {
                        "text": "outcomemarker retry dependency build",
                        "project": "alpha",
                    }
                    if status == "failed":
                        approach["evidence"] = "test exit 1; password=supersecret123"
                        reply = json.loads(
                            mcp(
                                "memory_remember",
                                dict(args, approach=approach, source_ids=[plain["id"]]),
                            )
                        )
                    else:
                        code, reply = request(
                            "/add",
                            dict(args, metadata={
                                "chunk_type": "manual",
                                "importance": "knowledge",
                                "approach": approach,
                            }),
                        )
                        assert code == 201, reply
                    ids.append(reply["id"])
                    row = get(reply["id"])
                    assert row["approach"]["status"] == status
                    assert (
                        row["approach"]["assertion"] == "caller_reported_not_verified"
                    )
                    assert row["approach"]["evidence_status"] == (
                        "provided_unverified" if status == "failed" else "not_provided"
                    )
                    assert "supersecret123" not in json.dumps(row)
                    assert (
                        json.loads(mcp("memory_get", {"id": reply["id"]}))["approach"]
                        == row["approach"]
                    )
                assert len(set(ids)) == 3
                # A plain caller must not inherit a prior outcome's status/evidence.
                code, plain_after = request(
                    "/add",
                    {"text": "outcomemarker retry dependency build", "project": "alpha"},
                )
                assert code == 201, plain_after
                assert plain_after["id"] not in ids, plain_after
                assert get(plain_after["id"])["approach"] is None
                code, _ = request(
                    "/update",
                    {"id": plain_after["id"], "text": "outcomemarker ordinary edit"},
                )
                assert code == 200
                assert set(ids) <= {r["id"] for r in search()}
                assert search("beta") == []
                text = mcp(
                    "memory_search",
                    {"query": "outcomemarker", "project": "alpha", "n_results": 50},
                )
                for status in [
                    "proposed",
                    "failed",
                    "reported_success",
                    "provided_unverified",
                    "not_provided",
                ]:
                    assert status in text, text
                assert "supersecret123" not in text
                # Same invalid nested contract on both transports, no accepted write.
                for bad in [
                    {},
                    {"status": "success", "applicability": "Linux"},
                    {"status": "failed", "applicability": "  "},
                    {"status": "failed", "applicability": 1},
                    {"status": "failed", "applicability": "Linux", "evidence": []},
                    {"status": "failed", "applicability": "Linux", "verified": True},
                    {"status": "failed", "applicability": "a" * 2049},
                    {
                        "status": "failed",
                        "applicability": "Linux",
                        "evidence": "e" * 4097,
                    },
                ]:
                    code, _ = request(
                        "/add",
                        {
                            "text": "invalid",
                            "project": "alpha",
                            "metadata": {"approach": bad},
                        },
                    )
                    assert code in (400, 422), (code, bad)
                    mcp(
                        "memory_remember",
                        {"text": "invalid", "project": "alpha", "approach": bad},
                        error=True,
                    )
                    code, _ = request(
                        "/update", {"id": ids[1], "metadata": {"approach": bad}}
                    )
                    assert code in (400, 422), code
                original = get(ids[1])
                assert original["provenance"]["source_ids"] == [plain["id"]]
                code, _ = request("/update", {"id": ids[1], "importance": "decision"})
                assert code == 200 and get(ids[1])["approach"] == original["approach"]
                code, _ = request(
                    "/update", {"id": ids[1], "text": original["document"]}
                )
                assert code == 200
                code, _ = request(
                    "/update",
                    {
                        "id": ids[1],
                        "text": "changed",
                        "project": "beta",
                        "metadata": {
                            "approach": {
                                "status": "reported_success",
                                "applicability": "changed",
                            }
                        },
                    },
                )
                assert code == 409
                assert get(ids[1])["project"] == "alpha"
                assert get(ids[1])["approach"] == original["approach"]
                assert get(ids[1])["document"] == original["document"]
                assert search("beta") == []
                # Explicit metadata replacement redacts; summary is bounded, get discloses detail.
                detailed = {
                    "status": "reported_success",
                    "applicability": "😀" * 2048,
                    "evidence": "password=supersecret123 " + "e" * 4000,
                }
                code, _ = request(
                    "/update", {"id": ids[2], "metadata": {"approach": detailed}}
                )
                assert code == 200
                summary = next(r for r in search() if r["id"] == ids[2])["approach"]
                assert summary["truncated"] and len(summary["applicability"]) == 256
                detail = get(ids[2])["approach"]
                assert not detail["truncated"] and len(detail["applicability"]) == 2048
                assert "supersecret123" not in json.dumps(detail)
                assert detail["evidence_status"] == "provided_unverified"
                # Supersession correction uses existing same-project boundaries.
                args = {
                    "text": "outcomemarker corrected workaround",
                    "project": "beta",
                    "approach": {
                        "status": "reported_success",
                        "applicability": "Linux",
                        "evidence": "caller says test passed",
                    },
                    "supersedes": ids[1],
                }
                mcp("memory_remember", args, error=True)
                args["project"] = "alpha"
                corrected = json.loads(mcp("memory_remember", args))["id"]
                visible = {r["id"] for r in search()}
                assert corrected in visible and ids[1] not in visible
                assert get(corrected)["provenance"]["supersedes"] == ids[1]
                assert get(ids[1])["approach"]["status"] == "failed"
                # Context prompt preserves distinctions and trust disclosure.
                code, context = request(
                    "/context",
                    {
                        "query": "outcomemarker",
                        "project": "alpha",
                        "n_results": 10,
                        "max_chars": 12000,
                    },
                )
                assert code == 200, context
                assert "caller_reported_not_verified" in context["prompt"], context
                # Both HTTP-created outcomes must be eligible for durable recall.
                context_ids = {r["id"] for r in context["memories"]}
                assert {ids[0], ids[2]} <= context_ids, context
                assert all(id in context["prompt"] for id in [ids[0], ids[2]])
                mcp("memory_delete", {"id": ids[0]})
                assert ids[0] not in {r["id"] for r in search()}
                code, after_delete = request(
                    "/context", {"query": "outcomemarker", "project": "alpha"}
                )
                assert code == 200
                assert ids[0] not in {r["id"] for r in after_delete["memories"]}
                print(
                    "PASS: isolated HTTP/MCP old/new clients, statuses, evidence, invalid metadata, bounds/redaction, update atomicity, correction and project isolation"
                )
            except Exception:
                log.flush()
                log.seek(0)
                print(log.read()[-5000:])
                raise
            finally:
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=10)


if __name__ == "__main__":
    run()
