#!/usr/bin/env python3
"""Retrieval-only benchmark for memnest (LoCoMo, LongMemEval-S). Standard library only.

    python3 benchmarks/retrieval/run.py --binary ~/.local/bin/memnest --dataset locomo
    python3 benchmarks/retrieval/run.py --binary ~/.local/bin/memnest --dataset longmemeval --sample 150

Starts a scratch memnest, ingests one memory per dialog turn, sends each question
unchanged to /search, computes recall/MRR/NDCG without any LLM, writes
results/<date>-v<version>-<dataset>[-tag].json, and deletes the scratch instance.
"""

import argparse
import json
import os
import platform
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import build  # noqa: E402
import datasets  # noqa: E402
import instance  # noqa: E402
import metrics  # noqa: E402

N_RESULTS = 10  # largest K reported; one request per question
DEPTH_RESULTS = 50  # API maximum, used only as a diagnostic depth probe


def log(message):
    print(message, file=sys.stderr, flush=True)


def metadata_for(turn, session_id):
    """Metadata the transcript watcher sends (core/src/watch.rs post_event)."""
    meta = {
        "chunk_type": "auto_log",
        "importance": "log",
        "session_id": session_id,
        "source": "bench.transcript",
        "adapter": "bench",
        "event_id": turn["event_id"],
        "sequence": 1,
        "total": 1,
    }
    if turn.get("role"):
        meta["role"] = turn["role"]
    return meta


def ingest_group(client, group):
    """POST /add once per turn, in conversation order. Returns (id->turn key, seconds)."""
    id_to_key = {}
    started = time.perf_counter()
    for turn in group["turns"]:
        status, body = client.post(
            "/add",
            {
                "text": turn["text"],
                "project": group["project"],
                "metadata": metadata_for(turn, turn["session"]),
            },
        )
        if status != 201 or body.get("status") != "succeeded":
            raise RuntimeError(f"/add failed for {turn['key']}: {status} {body}")
        id_to_key[body["id"]] = turn["key"]
    return id_to_key, time.perf_counter() - started


def search(client, query, project, n_results):
    started = time.perf_counter()
    status, body = client.post(
        "/search", {"query": query, "project": project, "n_results": n_results}
    )
    wall_ms = (time.perf_counter() - started) * 1000
    if status != 200:
        raise RuntimeError(f"/search failed: {status} {body}")
    return body["results"], wall_ms, body.get("elapsed_ms")


def run_group(client, group, dataset):
    id_to_key, ingest_seconds = ingest_group(client, group)
    session_of = {t["key"]: t["session"] for t in group["turns"]}
    records = []
    for question in group["questions"]:
        results, wall_ms, server_ms = search(
            client, question["question"], group["project"], N_RESULTS
        )
        ranked = [id_to_key.get(r["id"], "unknown:" + r["id"]) for r in results]
        deep, _, _ = search(
            client, question["question"], group["project"], DEPTH_RESULTS
        )
        deep_ranked = [id_to_key.get(r["id"], "unknown:" + r["id"]) for r in deep]
        record = {
            "type": "question",
            **question,
            "ranked": ranked,
            "ranked_sessions": metrics.dedupe_keep_order(
                [session_of.get(k, "unknown") for k in ranked]
            ),
            "scores": [round(r["score"], 5) for r in results],
            "n_returned": len(ranked),
            "ranked_depth50": deep_ranked,
            "ranked_depth50_sessions": metrics.dedupe_keep_order(
                [session_of.get(k, "unknown") for k in deep_ranked]
            ),
            "latency_ms": round(wall_ms, 2),
            "server_elapsed_ms": server_ms,
            "project": group["project"],
            "project_memories": len(group["turns"]),
        }
        records.append(record)
    return records, {
        "type": "group",
        "project": group["project"],
        "memories": len(group["turns"]),
        "ingest_seconds": round(ingest_seconds, 3),
    }


# ------------------------------------------------------------------ summary


def locomo_metrics(records):
    def block(rows):
        out: dict = {"n": len(rows)}
        for k in metrics.KS:
            out[f"any_recall@{k}"] = metrics.mean(
                metrics.any_hit_at_k(r["ranked"], r["relevant"], k) for r in rows
            )
            out[f"all_recall@{k}"] = metrics.mean(
                metrics.all_hit_at_k(r["ranked"], r["relevant"], k) for r in rows
            )
        out["mrr@10"] = metrics.mean(
            metrics.reciprocal_rank(r["ranked"], r["relevant"]) for r in rows
        )
        out["any_recall@50"] = metrics.mean(
            metrics.any_hit_at_k(r["ranked_depth50"], r["relevant"], 50) for r in rows
        )
        return out

    scored = [r for r in records if r["relevant"]]
    result = {
        "questions_total": len(records),
        "questions_without_evidence_skipped": len(records) - len(scored),
        "overall_all_categories": block(scored),
        "overall_categories_1_to_4": block(
            [r for r in scored if r["category_number"] != 5]
        ),
        "per_category": {},
    }
    for category in build.LOCOMO_CATEGORIES.values():
        rows = [r for r in scored if r["category"] == category]
        if rows:
            result["per_category"][category] = block(rows)
    return result


def lme_metrics(records):
    def block(rows):
        out: dict = {"n": len(rows)}
        for k in metrics.KS:
            out[f"session_any_recall@{k}"] = metrics.mean(
                metrics.any_hit_at_k(r["ranked_sessions"], r["relevant_sessions"], k)
                for r in rows
            )
            out[f"session_all_recall@{k}"] = metrics.mean(
                metrics.all_hit_at_k(r["ranked_sessions"], r["relevant_sessions"], k)
                for r in rows
            )
        out["session_ndcg@10"] = metrics.mean(
            metrics.ndcg_at_k(r["ranked_sessions"], r["relevant_sessions"], 10)
            for r in rows
        )
        out["session_mrr@10"] = metrics.mean(
            metrics.reciprocal_rank(r["ranked_sessions"], r["relevant_sessions"])
            for r in rows
        )
        with_turns = [r for r in rows if r["relevant"]]
        out["turn_level_n"] = len(with_turns)
        for k in metrics.KS:
            out[f"turn_any_recall@{k}"] = metrics.mean(
                metrics.any_hit_at_k(r["ranked"], r["relevant"], k) for r in with_turns
            )
            out[f"turn_all_recall@{k}"] = metrics.mean(
                metrics.all_hit_at_k(r["ranked"], r["relevant"], k) for r in with_turns
            )
        out["session_any_recall@50turns"] = metrics.mean(
            metrics.any_hit_at_k(
                r["ranked_depth50_sessions"], r["relevant_sessions"], 50
            )
            for r in rows
        )
        return out

    scored = [r for r in records if not r["abstention"]]
    result = {
        "questions_total": len(records),
        "abstention_questions_skipped": len(records) - len(scored),
        "overall": block(scored),
        "per_question_type": {},
    }
    for question_type in sorted({r["category"] for r in scored}):
        result["per_question_type"][question_type] = block(
            [r for r in scored if r["category"] == question_type]
        )
    return result


def performance(group_rows, records, workers):
    memories = sum(g["memories"] for g in group_rows)
    seconds = sum(g["ingest_seconds"] for g in group_rows)
    latencies = [r["latency_ms"] for r in records]
    server = [r["server_elapsed_ms"] for r in records if r["server_elapsed_ms"] is not None]
    return {
        "workers": workers,
        "ingest_memories": memories,
        "ingest_seconds_sum": round(seconds, 1),
        "ingest_memories_per_second": round(memories / seconds, 2) if seconds else None,
        "search_requests_timed": len(latencies),
        "search_latency_ms_p50": metrics.percentile(latencies, 0.5),
        "search_latency_ms_p95": metrics.percentile(latencies, 0.95),
        "search_latency_ms_mean": round(sum(latencies) / len(latencies), 2) if latencies else None,
        "search_server_ms_p50": metrics.percentile(server, 0.5),
        "search_server_ms_p95": metrics.percentile(server, 0.95),
        "latency_note": "wall time of one POST /search (n_results=10) from the client, "
        "searches issued serially after ingest of that project; includes query embedding",
    }


def hardware():
    cpu = "unknown"
    with open("/proc/cpuinfo", encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    mem_kb = 0
    with open("/proc/meminfo", encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("MemTotal"):
                mem_kb = int(line.split()[1])
                break
    return {
        "cpu": cpu,
        "logical_cpus": os.cpu_count(),
        "ram_gib": round(mem_kb / 1048576, 1),
        "os": platform.platform(),
        "gpu": "none used (CPU-only ONNX inference inside memnest)",
    }


def load_average():
    """1-minute load average of the whole machine (other jobs distort timings)."""
    with open("/proc/loadavg", encoding="utf-8") as handle:
        return float(handle.read().split()[0])


def sha256_of(path):
    return datasets.sha256_file(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, help="memnest binary under test")
    parser.add_argument("--dataset", required=True, choices=["locomo", "longmemeval"])
    parser.add_argument(
        "--no-date",
        action="store_true",
        help="do not put the session date in the memory text (watcher-faithful: "
        "the real watcher stores no date in the text)",
    )
    parser.add_argument("--sample", type=int, default=0,
                        help="LongMemEval only: fixed-seed stratified sample of N non-abstention questions (0 = all)")
    parser.add_argument("--seed", type=int, default=1110)
    parser.add_argument("--max-groups", type=int, default=0,
                        help="smoke tests: stop after this many groups")
    parser.add_argument("--shard", default="0/1", help="i/N: process groups with index %% N == i")
    parser.add_argument("--tag", default="", help="suffix for the result file name")
    parser.add_argument("--out-dir", default=os.path.join(HERE, "results"))
    parser.add_argument("--checkpoint", default="",
                        help="JSONL checkpoint; finished groups are skipped on rerun")
    parser.add_argument("--port", type=int, default=0)
    args = parser.parse_args()

    shard_index, shard_count = (int(x) for x in args.shard.split("/"))
    date_in_text = not args.no_date
    data, provenance = datasets.load(args.dataset)
    sample_indices: list = []
    if args.dataset == "locomo":
        groups = build.locomo_groups(data, date_in_text)
    else:
        if args.sample:
            sample_indices = build.stratified_sample(data, args.sample, args.seed)
        else:
            sample_indices = list(range(len(data)))
        groups = build.lme_groups(data, date_in_text, sample_indices)
    groups = [g for i, g in enumerate(groups) if i % shard_count == shard_index]
    if args.max_groups:
        groups = groups[: args.max_groups]

    done = {}
    checkpoint = args.checkpoint
    if checkpoint and os.path.exists(checkpoint):
        with open(checkpoint, encoding="utf-8") as handle:
            for line in handle:
                row = json.loads(line)
                done.setdefault(row.get("project"), []).append(row)

    all_records, group_rows = [], []
    with instance.Instance(args.binary, args.port or None) as scratch:
        health = scratch.health()
        client = scratch.client
        if client is None:
            raise RuntimeError("scratch instance has no client")
        log(f"memnest {health['version']} on port {scratch.port}; {len(groups)} groups")
        # Load the embedding model before timing anything.
        client.post("/search", {"query": "warm up", "project": "warmup"})
        run_started = time.time()
        load_start = load_average()
        load_samples = [load_start]
        for number, group in enumerate(groups, start=1):
            if group["project"] in done:
                rows = done[group["project"]]
                all_records += [r for r in rows if r["type"] == "question"]
                group_rows += [r for r in rows if r["type"] == "group"]
                continue
            records, group_row = run_group(client, group, args.dataset)
            all_records += records
            group_rows.append(group_row)
            if checkpoint:
                with open(checkpoint, "a", encoding="utf-8") as handle:
                    for row in [group_row, *records]:
                        handle.write(json.dumps({**row, "project": group["project"]}) + "\n")
            elapsed = time.time() - run_started
            load_samples.append(load_average())
            rate = group_row["memories"] / group_row["ingest_seconds"]
            log(
                f"[{number}/{len(groups)}] {group['project']}: {group_row['memories']} memories "
                f"@ {rate:.1f}/s; elapsed {elapsed/60:.1f} min"
            )
        binary_path = scratch.binary
        version = str(health["version"])
        embed_model = str(health["embed_model"])

    if args.dataset == "locomo":
        summary = locomo_metrics(all_records)
        scored_n = summary["questions_total"] - summary["questions_without_evidence_skipped"]
    else:
        summary = lme_metrics(all_records)
        scored_n = summary["overall"]["n"]
    sampled = bool(args.sample) and args.dataset == "longmemeval"
    result = {
        "benchmark": "memnest retrieval-only (no LLM): " + args.dataset,
        "run_date": time.strftime("%Y-%m-%d"),
        "binary": {
            "path": binary_path,
            "sha256": sha256_of(binary_path),
            "version_from_health": version,
            "embed_model": embed_model,
        },
        "dataset": provenance,
        "config": {
            "memory_format": "one memory per dialog turn: "
            + ("'[<session date>] <speaker> said: <text>'" if date_in_text else "'<speaker> said: <text>'")
            if args.dataset == "locomo"
            else "one memory per dialog turn: "
            + ("'[<session date>] ' + " if date_in_text else "")
            + "'User said: ' / 'Assistant answered: ' + content",
            "date_in_text": date_in_text,
            "add_metadata": "chunk_type=auto_log, importance=log, source=bench.transcript, adapter=bench, "
            "session_id=<dataset session>, event_id=<dataset turn id>, sequence=1, total=1",
            "project_scope": "one project per conversation (LoCoMo) / per question haystack (LongMemEval)",
            "search_request": {"query": "<question text unchanged>", "project": "<own project>",
                                "n_results": N_RESULTS, "other_fields": "server defaults"},
            "depth_probe_request": {"n_results": DEPTH_RESULTS, "purpose": "diagnostic only, not used in headline metrics except where labelled @50"},
            "server_defaults": "no env overrides (MEMNEST_* removed); mmr_lambda=0.5, distance_cutoff=0.7, low_relevance_fallback=3",
            "sampled": sampled,
            "sample_size": args.sample if sampled else None,
            "sample_seed": args.seed if sampled else None,
            "sample_question_ids": [data[i]["question_id"] for i in sample_indices] if sampled else None,
            "shard": args.shard,
            "max_groups": args.max_groups or None,
            "questions_scored": scored_n,
        },
        "hardware": hardware(),
        "performance": {
            **performance(group_rows, all_records, shard_count),
            "machine_load_1min_at_start": load_start,
            "machine_load_1min_max_after_groups": max(load_samples),
            "machine_load_1min_mean_after_groups": round(sum(load_samples) / len(load_samples), 2),
            "logical_cpus": os.cpu_count(),
        },
        "metrics": summary,
        "unknown_evidence_dropped": sum(
            g.get("unknown_evidence_dropped", 0) for g in groups
        ),
        "per_question": [
            {k: v for k, v in r.items() if k not in ("type",)} for r in all_records
        ],
    }
    os.makedirs(args.out_dir, exist_ok=True)
    name = f"{result['run_date']}-v{version}-{args.dataset}{('-' + args.tag) if args.tag else ''}.json"
    out_path = os.path.join(args.out_dir, name)
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(result, handle, ensure_ascii=False, indent=1)
    log(f"wrote {out_path}")
    print(json.dumps({"metrics": summary, "performance": result["performance"]}, indent=1))


if __name__ == "__main__":
    main()
