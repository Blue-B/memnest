#!/usr/bin/env python3
"""CLI diagnostic contracts against disposable loopback test doubles (no DB/model)."""

import json
import os
import secrets
import socket
import subprocess
import tempfile
import threading
import time
from contextlib import suppress
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "core/target/debug/memnest"


def main():
    with tempfile.TemporaryDirectory(prefix="memnest-status-") as temporary:
        data = Path(temporary) / "must-not-exist"
        env = {k: v for k, v in os.environ.items() if not k.startswith("MEMNEST_")}
        token = secrets.token_hex(24)
        env["MEMNEST_TOKEN"] = token
        seen = []
        mode = "ok"
        private_body = f"server-private-{secrets.token_hex(12)}"
        server_path = "/private/installation/memory.db"

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, format, *args):
                pass

            def do_GET(self):
                self.reply()

            def do_POST(self):
                try:
                    length = int(self.headers["Content-Length"])
                    body = json.loads(self.rfile.read(length))
                except (TypeError, ValueError):
                    self.send_error(400)
                    return
                assert body["method"] == "tools/list", body
                self.reply()

            def reply(self):
                seen.append(
                    (self.command, self.path, self.headers.get("Authorization"))
                )
                code = 200
                if mode == "slow":
                    time.sleep(6)
                if mode == "unauthorized":
                    code = 401
                elif mode == "redirect":
                    code = 302
                self.send_response(code)
                self.send_header("Location", "/must-not-follow")
                self.end_headers()
                fields = ["id", "offset", "max_chars"] if mode != "old" else ["id"]
                body = (
                    {
                        "status": "ok",
                        "version": private_body + token,
                        "data_dir": server_path,
                        "embedding": {"loaded": False},
                        "index": {
                            "pending_operations": 2,
                            "rebuild_required": True,
                        },
                    }
                    if self.path == "/health"
                    else {
                        "result": {
                            "tools": [
                                {
                                    "name": "memory_get",
                                    "description": private_body + server_path + token,
                                    "inputSchema": {
                                        "properties": dict.fromkeys(fields, {})
                                    },
                                },
                                {
                                    "name": "memory_search",
                                    "inputSchema": {
                                        "properties": {"query": {}, "project": {}}
                                    },
                                },
                            ]
                        }
                    }
                )
                payload = json.dumps(body).encode()
                if mode == "invalid":
                    payload = (private_body + server_path + token).encode()
                if mode == "large":
                    payload = b"x" * (256 * 1024 + 1)
                with suppress(BrokenPipeError, ConnectionResetError):
                    self.wfile.write(payload)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()

        def invoke(port):
            start = time.monotonic()
            result = subprocess.run(
                [
                    str(BINARY),
                    "--data-dir",
                    str(data),
                    "--port",
                    str(port),
                    "status",
                    "--diagnose",
                ],
                env=env,
                capture_output=True,
                text=True,
                timeout=8,
            )
            assert time.monotonic() - start < 7
            assert not data.exists()
            for private in [token, private_body, server_path, str(data)]:
                assert private not in result.stdout + result.stderr
            return result

        try:
            for mode, expected, code in [
                ("ok", "capabilities: scoped search", 0),
                ("unauthorized", "authentication failure", 1),
                ("old", "MCP capability mismatch", 1),
                ("invalid", "invalid JSON contract", 1),
                ("large", "exceeds 256 KiB", 1),
                ("redirect", "HTTP 302", 1),
                ("slow", "timed out", 1),
            ]:
                seen.clear()
                result = invoke(server.server_port)
                assert result.returncode == code and expected in result.stdout, result
                if mode == "ok":
                    assert "embedding: lazy" in result.stdout, result
                    assert "index: 2 pending operation(s), rebuild_required=true" in result.stdout, result
                assert all(
                    path in ["/health", "/mcp"] and auth == f"Bearer {token}"
                    for _, path, auth in seen
                ), seen
                print(
                    f"PASS {mode}: exit={code}, {expected}; no data directory, token/body/path disclosure"
                )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
        # Reserved but non-listening port cannot belong to an ambient service.
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            result = invoke(sock.getsockname()[1])
            assert result.returncode == 1 and "service unreachable" in result.stdout, (
                result
            )
        print("PASS unreachable")


if __name__ == "__main__":
    main()
