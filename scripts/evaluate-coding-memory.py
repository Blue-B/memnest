#!/usr/bin/env python3
"""Fixed relevance judgments, actual disposable HTTP service, no downloads or LLM judge.

Usage: python3 scripts/evaluate-coding-memory.py --output /tmp/evaluation.json
Quality scores are measurements, not pass gates; isolation/currentness are invariants.
"""

import argparse
import json
import os
import re
import secrets
import socket
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def tokens(text):
    return set(re.findall(r"\w+", text.lower()))


def summarize(rows, key):
    positive = [r for r in rows if r["relevant"]]
    negative = [r for r in rows if not r["relevant"]]
    ranks = [
        next((i + 1 for i, k in enumerate(r[key]) if k in r["relevant"]), 0)
        for r in positive
    ]
    return {
        "positive_cases": len(positive),
        "hit_at_1": sum(n == 1 for n in ranks) / len(ranks),
        "recall_at_5": sum(n > 0 for n in ranks) / len(ranks),
        "mrr_at_5": sum(1 / n if n else 0 for n in ranks) / len(ranks),
        "negative_cases": len(negative),
        "negative_empty_rate": sum(not r[key] for r in negative) / len(negative),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    fixture_path = ROOT / "scripts/fixtures/coding-memory.json"
    fixture = json.loads(fixture_path.read_text())
    cache = ROOT / "core/target/test-model-cache"
    assert (cache / "models--intfloat--multilingual-e5-base/refs/main").is_file(), (
        "existing cache required"
    )
    binary = ROOT / "core/target/debug/memnest"
    with tempfile.TemporaryDirectory(prefix="memnest-evaluation-") as temporary:
        root = Path(temporary)
        data = root / "data"
        data.mkdir()
        (data / "models").symlink_to(cache, target_is_directory=True)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        env = {k: v for k, v in os.environ.items() if not k.startswith("MEMNEST_")}
        token = secrets.token_hex(24)
        env.update(
            HF_HUB_OFFLINE="1",
            MEMNEST_ARCHIVE="0",
            MEMNEST_TOKEN=token,
            HTTP_PROXY="http://127.0.0.1:1",
            HTTPS_PROXY="http://127.0.0.1:1",
            ALL_PROXY="http://127.0.0.1:1",
            NO_PROXY="127.0.0.1,localhost",
        )
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

        def request(path, body=None):
            req = urllib.request.Request(  # noqa: S310 - fixed disposable loopback base
                base + path,
                data=None if body is None else json.dumps(body).encode(),
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {token}",
                },
            )
            with opener.open(req, timeout=60) as response:
                return json.load(response)

        log_path = args.output.with_suffix(".daemon.log")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open("w") as log:
            child = subprocess.Popen(
                [
                    str(binary),
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
                deadline = time.monotonic() + 60
                while True:
                    assert child.poll() is None, "child exited; see daemon log"
                    try:
                        health = request("/health")
                        # Avoid accidentally testing another process after a port-allocation race.
                        assert health["data_dir"] == str(data), "wrong server identity"
                        break
                    except (OSError, TimeoutError):
                        assert time.monotonic() < deadline, "startup timeout"
                        time.sleep(0.2)
                cli = [
                    str(binary),
                    "--data-dir",
                    str(root / "absent"),
                    "--port",
                    str(port),
                    "status",
                    "--diagnose",
                ]
                diagnostic = subprocess.run(
                    cli, env=env, capture_output=True, text=True, timeout=10
                )
                assert (
                    diagnostic.returncode == 0
                    and "capabilities: scoped search" in diagnostic.stdout
                ), diagnostic
                assert not (root / "absent").exists(), "diagnostic created data dir"
                unauthorized = subprocess.run(
                    cli,
                    env=dict(env, MEMNEST_TOKEN=secrets.token_hex(24)),
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                assert (
                    unauthorized.returncode == 1
                    and "authentication failure" in unauthorized.stdout
                )
                ids = {}
                records = fixture["records"]
                for record in records:
                    metadata = {"memory_kind": "fact"}
                    if "supersedes" in record:
                        metadata["supersedes"] = ids[record["supersedes"]]
                    result = request(
                        "/add",
                        {
                            "text": record["text"],
                            "project": record["project"],
                            "metadata": metadata,
                        },
                    )
                    assert result["status"] == "succeeded", result
                    ids[record["key"]] = result["id"]
                reverse = {value: key for key, value in ids.items()}
                obsolete = {r["supersedes"] for r in records if "supersedes" in r}
                rows = []
                for case in fixture["cases"]:
                    start = time.monotonic()
                    result = request(
                        "/search",
                        {
                            "query": case["query"],
                            "project": case["project"],
                            "n_results": 5,
                        },
                    )
                    elapsed = (time.monotonic() - start) * 1000
                    found = [reverse[r["id"]] for r in result["results"]]
                    eligible = [
                        r
                        for r in records
                        if r["project"] == case["project"] and r["key"] not in obsolete
                    ]
                    assert set(found) <= {r["key"] for r in eligible}, (case, found)
                    # Transparent lexical/file baseline: Unicode token intersection count,
                    # ties by fixed fixture order, zero-overlap abstention, same visibility/scope.
                    scored = [
                        (len(tokens(case["query"]) & tokens(r["text"])), r["key"])
                        for r in eligible
                    ]
                    scored.sort(key=lambda pair: -pair[0])
                    baseline = [key for score, key in scored if score > 0][:5]
                    rows.append(
                        dict(
                            case,
                            service=found,
                            keyword=baseline,
                            latency_ms=round(elapsed, 3),
                            scores=[r.get("score") for r in result["results"]],
                        )
                    )
                latencies = sorted(r["latency_ms"] for r in rows)
                rss = next(
                    (
                        line.strip()
                        for line in Path(f"/proc/{child.pid}/status")
                        .read_text()
                        .splitlines()
                        if line.startswith("VmRSS:")
                    ),
                    "unavailable",
                )
                output = {
                    "fixture": str(fixture_path.relative_to(ROOT)),
                    "records": len(records),
                    "cases": rows,
                    "metrics": {
                        key: summarize(rows, key) for key in ["service", "keyword"]
                    },
                    "latency_ms": {
                        "p50": latencies[len(latencies) // 2],
                        "p95": latencies[int(len(latencies) * 0.95)],
                    },
                    "rss_after_queries": rss,
                    "health": {k: health[k] for k in ["version", "embed_model"]},
                    "diagnostic": diagnostic.stdout,
                    "unauthorized": unauthorized.stdout,
                    "limitations": "26 authored cases, 16 records, single ordered warm run. No paid LLM, competitor, answer-generation or general superiority evaluation. Token baseline has no morphology/stopword/ranking tuning.",
                }
                args.output.write_text(
                    json.dumps(output, ensure_ascii=False, indent=2) + "\n"
                )
                print(
                    json.dumps(
                        {
                            "metrics": output["metrics"],
                            "latency_ms": output["latency_ms"],
                            "rss": rss,
                        },
                        indent=2,
                    )
                )
            finally:
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=10)


if __name__ == "__main__":
    main()
