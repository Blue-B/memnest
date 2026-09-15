#!/usr/bin/env python3
"""Disposable wrong-identity, redirect and ambient-proxy regression."""
import importlib.util
import json
import os
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "lifecycle", Path(__file__).with_name("test-memory-lifecycle.py")
)
assert spec is not None and spec.loader is not None
lifecycle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lifecycle)


class Safety(unittest.TestCase):
    def test_wrong_identity_before_mutation_and_proxy_bypass(self):
        calls = []

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                calls.append((self.path, self.headers.get("Authorization")))
                self.send_response(200)
                self.end_headers()
                self.wfile.write(json.dumps({"data_dir": "/wrong"}).encode())

            def do_POST(self):
                calls.append(("MUTATION", None))
                self.send_response(500)
                self.end_headers()

            def log_message(self, format, *args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        base = f"http://127.0.0.1:{server.server_port}"

        def request(path, body=None):
            return lifecycle.local_request(base, "test-only", path, body)

        try:
            with patch.dict(os.environ, {
                "http_proxy": "http://127.0.0.1:1",
                "HTTP_PROXY": "http://127.0.0.1:1",
                "no_proxy": "", "NO_PROXY": "",
            }), self.assertRaisesRegex(AssertionError, "wrong server identity"):
                # Same startup gate, with a live wrong-identity responder.
                if lifecycle.ready(request, Path("/expected")):
                    request("/add", {"text": "must not be sent"})
            self.assertEqual(calls, [("/health", "Bearer test-only")])
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

    def test_authentication_and_redirect_fail_closed(self):
        with self.assertRaisesRegex(AssertionError, "authentication"):
            lifecycle.ready(lambda _: (401, {}), Path("/expected"))
        with self.assertRaisesRegex(AssertionError, "must not redirect"):
            lifecycle.NoRedirect().redirect_request(
                None, None, 302, "", {}, "http://127.0.0.1:1"
            )
        with self.assertRaises(AssertionError):
            lifecycle.local_request("file:///tmp", "", "/health")


if __name__ == "__main__":
    unittest.main()
