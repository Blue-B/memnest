#!/usr/bin/env python3
"""Merge LongMemEval shard results into one result file.

    python3 benchmarks/retrieval/merge.py --out results/<date>-v0.5.1-longmemeval.json \
        [--latency-from serial-run.json] shard0.json shard1.json

Metrics are recomputed from the per-question records. Search latency measured
while shards ran in parallel is not reported; pass --latency-from with a result
file from a serial run (one instance, one request at a time) to report latency.
"""

import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import run  # noqa: E402


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("shards", nargs="+")
    parser.add_argument("--out", required=True)
    parser.add_argument("--latency-from", default="")
    args = parser.parse_args()

    loaded = []
    for path in args.shards:
        with open(path, encoding="utf-8") as handle:
            loaded.append(json.load(handle))
    first = loaded[0]
    records = [dict(r, type="question") for part in loaded for r in part["per_question"]]
    records.sort(key=lambda r: r["id"])
    metrics_block = run.lme_metrics(records)

    memories = sum(p["performance"]["ingest_memories"] for p in loaded)
    seconds = [p["performance"]["ingest_seconds_sum"] for p in loaded]
    # Parts that only re-asked questions against kept data dirs (run.py --kept-only)
    # ingested nothing and were run one after another.
    kept = memories == 0
    performance = {
        "parallel_instances": 1 if kept else len(loaded),
        "ingest_memories": memories,
        "ingest_seconds_per_instance": seconds,
        "ingest_memories_per_second_per_instance": [
            p["performance"]["ingest_memories_per_second"] for p in loaded
        ],
        "ingest_memories_per_second_all_instances": round(memories / max(seconds), 2) if max(seconds) else None,
        "ingest_note": "no ingest: every part asked its questions against a kept data directory"
        if kept
        else "instances ran in parallel on the same 10-thread CPU while other "
        "processes were also using the machine; per-instance rate is the figure to compare",
    }
    if args.latency_from:
        with open(args.latency_from, encoding="utf-8") as handle:
            serial = json.load(handle)
        for key in (
            "search_requests_timed", "search_latency_ms_p50", "search_latency_ms_p95",
            "search_latency_ms_mean", "search_server_ms_p50", "search_server_ms_p95",
        ):
            performance[key] = serial["performance"][key]
        performance["serial_ingest_memories_per_second"] = serial["performance"][
            "ingest_memories_per_second"
        ]
        performance["latency_note"] = (
            "from a separate serial run (one instance, one request at a time) over the "
            f"first {serial['performance']['search_requests_timed']} questions of the same sample; "
            "wall time of POST /search n_results=10 including query embedding"
        )
    else:
        performance["latency_note"] = (
            "not reported: the parts were timed separately"
            if kept
            else "not reported: shards ran in parallel"
        )

    config = dict(first["config"])
    config["shard"] = f"merged {len(loaded)} shards"
    config["questions_scored"] = metrics_block["overall"]["n"]
    result = {
        key: first[key] for key in ("benchmark", "run_date", "binary", "dataset", "hardware")
    }
    result.update(
        config=config,
        performance=performance,
        metrics=metrics_block,
        per_question=[{k: v for k, v in r.items() if k != "type"} for r in records],
    )
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(result, handle, ensure_ascii=False, indent=1)
    print("wrote", args.out, "questions", len(records))


if __name__ == "__main__":
    main()
