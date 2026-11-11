"""Start and stop a scratch memnest instance with a throw-away data directory.

Nothing under ~/.memnest is modified: the embedding model cache is
hard-link-copied (`cp -al`) into the scratch data dir, so it costs no disk space
and no download, and the scratch dir is deleted on exit.

With `data_dir` the instance runs on that directory and leaves it in place, so
data ingested once can be searched again by another binary (see run.py
--data-dir). The live data directory is refused.
"""

import http.client
import json
import os
import shutil
import signal
import socket
import subprocess
import tempfile
import time

PORTS = range(3981, 3990)
SOURCE_MODELS = os.path.expanduser("~/.memnest/models")


def _port_free(port):
    with socket.socket() as probe:
        return probe.connect_ex(("127.0.0.1", port)) != 0


class Client:
    """Minimal keep-alive JSON client for one instance."""

    def __init__(self, port):
        self.port = port
        self.conn = http.client.HTTPConnection("127.0.0.1", port, timeout=300)

    def request(self, method, path, body=None):
        payload = None if body is None else json.dumps(body).encode()
        headers = {"Content-Type": "application/json"} if payload else {}
        for attempt in (0, 1):
            try:
                self.conn.request(method, path, payload, headers)
                response = self.conn.getresponse()
                data = response.read()
                return response.status, json.loads(data) if data else None
            except (http.client.HTTPException, ConnectionError, TimeoutError):
                self.conn.close()
                self.conn = http.client.HTTPConnection(
                    "127.0.0.1", self.port, timeout=300
                )
                if attempt:
                    raise
        raise ConnectionError("request failed")  # not reached: the second attempt returns or raises

    def get(self, path):
        return self.request("GET", path)

    def post(self, path, body):
        return self.request("POST", path, body)


def _wait_healthy(proc, client, log_path, timeout=300):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if proc.poll() is not None:
            with open(log_path, errors="replace") as handle:
                tail = handle.read()[-2000:]
            raise RuntimeError(f"memnest exited early ({proc.returncode}); log: {tail}")
        try:
            status, body = client.get("/health")
            if status == 200 and body.get("status") == "ok":
                return body
        except (OSError, http.client.HTTPException):
            pass
        time.sleep(0.5)
    raise TimeoutError("memnest /health did not become ready")


def _refuse_live_data_dir(data_dir):
    live = os.path.realpath(os.path.dirname(SOURCE_MODELS))
    path = os.path.realpath(data_dir)
    if os.path.commonpath([live, path]) == live:
        raise ValueError(f"refusing to run a benchmark instance on {path}")


class Instance:
    def __init__(self, binary, port=None, data_dir=None):
        self.binary = os.path.abspath(os.path.expanduser(binary))
        self.port = port
        self.data_dir = os.path.abspath(os.path.expanduser(data_dir)) if data_dir else None
        self.root = None
        self.proc = None
        self.log_path = None
        self.client = None

    def __enter__(self):
        try:
            self.start()
        except BaseException:
            self.stop()  # a failed start must not leak the temp dir or process
            raise
        return self

    def __exit__(self, *exc):
        self.stop()

    def start(self):
        if not os.path.isfile(self.binary):
            raise FileNotFoundError(self.binary)
        ports = [self.port] if self.port else list(PORTS)
        port = next((p for p in ports if _port_free(p)), None)
        if port is None:
            raise RuntimeError(f"no free port in {ports}")
        self.port = port
        if self.data_dir:
            _refuse_live_data_dir(self.data_dir)
        self.root = tempfile.mkdtemp(prefix="memnest-bench-")
        data_dir = self.data_dir or os.path.join(self.root, "data")
        os.makedirs(data_dir, exist_ok=True)
        if os.path.isdir(SOURCE_MODELS) and not os.path.exists(os.path.join(data_dir, "models")):
            subprocess.run(
                ["cp", "-al", SOURCE_MODELS, os.path.join(data_dir, "models")],
                check=True,
            )
        log_path = os.path.join(self.root, "server.log")
        self.log_path = log_path
        env = {k: v for k, v in os.environ.items() if not k.startswith("MEMNEST_")}
        command = [
            self.binary,
            "--host", "127.0.0.1",
            "--port", str(port),
            "--data-dir", data_dir,
        ]
        with open(log_path, "wb") as log:
            proc = subprocess.Popen(
                command,
                stdout=log,
                stderr=subprocess.STDOUT,
                env=env,
                start_new_session=True,
            )
        self.proc = proc
        client = Client(port)
        self.client = client
        _wait_healthy(proc, client, log_path)

    def health(self):
        if self.client is None:
            raise RuntimeError("instance not started")
        _, body = self.client.get("/health")
        if not isinstance(body, dict):
            raise RuntimeError("/health returned no JSON object")
        return body

    def stop(self):
        if self.proc and self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        if self.root and os.path.isdir(self.root):
            shutil.rmtree(self.root, ignore_errors=True)
        self.proc = None
        self.root = None
