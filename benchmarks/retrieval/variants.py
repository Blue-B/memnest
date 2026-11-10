#!/usr/bin/env python3
"""Measure query-side / request-side variants on a fixed LoCoMo subset (no product code changes).

    python3 benchmarks/retrieval/variants.py --binary ~/.local/bin/memnest [--conversations 0,1,2,3]

Ingests the chosen conversations once into a scratch instance, then sends every
scored question several ways and compares recall against the unmodified request
on the same data (paired). Variants only use what the HTTP API already allows:
different n_results, and rewritten query text. None needs an LLM.
"""

import argparse
import json
import os
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import analyze_misses as am  # noqa: E402
import build  # noqa: E402
import datasets  # noqa: E402
import instance  # noqa: E402
import metrics  # noqa: E402
import run  # noqa: E402


def strip_names(question, names):
    out = question
    for name in names:
        out = re.sub(rf"\b{re.escape(name)}(?:'s)?\b", " ", out, flags=re.I)
    return re.sub(r"\s+", " ", out).strip(" ?") or question


def content_words(question):
    kept = [w for w in re.findall(r"[A-Za-z0-9']+", question) if w.lower() not in am.STOP]
    return " ".join(kept) or question


SERVER_STOPWORDS = {
    "a", "an", "and", "are", "as", "at", "be", "by", "do", "does", "for", "from", "how", "in",
    "is", "it", "of", "on", "or", "our", "should", "the", "this", "to", "was", "what", "when",
    "where", "which", "who", "why", "with",
}  # ENGLISH_STOPWORDS in core/src/search/mod.rs


def server_keywords(question):
    """What the server's extract_keywords(query, 2) keeps (English-only path), joined by spaces.
    Punctuation that breaks the BM25 query parser is dropped from the words."""
    tokens = re.split(r"[\s,;:!?'\"()\[\]{}]+", question)
    kept = [t for t in tokens if len(t) >= 2 and t.lower() not in SERVER_STOPWORDS]
    return " ".join(kept) or question


def syntax_safe(question):
    """Drop punctuation that tantivy's query parser treats as syntax ('s, quotes, brackets...)."""
    out = re.sub(r"['\u2019]s\b", "", question)
    return re.sub(r"\s+", " ", re.sub(r"[^\w\s]", " ", out)).strip() or question


def label_form(question, names):
    """Mimic the stored form '<Speaker> said: ...' so the query looks like a turn."""
    for name in names:
        if re.search(rf"\b{re.escape(name)}\b", question, re.I):
            return f"{name} said: {question}"
    return question


def rrf(rankings, k=60, top=10):
    score = {}
    for ranking in rankings:
        for position, key in enumerate(ranking, start=1):
            score[key] = score.get(key, 0.0) + 1.0 / (k + position)
    return [key for key, _ in sorted(score.items(), key=lambda item: -item[1])][:top]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--conversations", default="0,1,2,3")
    parser.add_argument("--no-date", action="store_true")
    parser.add_argument("--out", default=os.path.join(HERE, "results", f"{time.strftime('%Y-%m-%d')}-v0.5.1-locomo-variants.json"))
    parser.add_argument("--port", type=int, default=0)
    args = parser.parse_args()

    data, provenance = datasets.load("locomo")
    wanted = [int(x) for x in args.conversations.split(",")]
    groups = build.locomo_groups(data, not args.no_date)
    names_by_project = {
        g["project"]: [data[i]["conversation"]["speaker_a"], data[i]["conversation"]["speaker_b"]]
        for i, g in enumerate(groups)
    }
    groups = [g for i, g in enumerate(groups) if i in wanted]

    rows = []
    with instance.Instance(args.binary, args.port or None) as scratch:
        client = scratch.client
        if client is None:
            raise RuntimeError("scratch instance has no client")
        version = scratch.health()["version"]
        client.post("/search", {"query": "warm up", "project": "warmup"})
        for group in groups:
            id_to_key, seconds = run.ingest_group(client, group)
            print(f"ingested {group['project']} ({len(group['turns'])} turns, {seconds:.0f}s)", file=sys.stderr, flush=True)
            names = names_by_project[group["project"]]

            def ask(query, n, project=group["project"], keys=id_to_key):
                results, _, _ = run.search(client, query, project, n)
                return [keys.get(r["id"], "?") for r in results]

            for q in group["questions"]:
                if not q["relevant"]:
                    continue
                text = q["question"]
                base = ask(text, 10)
                nonames = ask(strip_names(text, names), 10)
                keywords = ask(content_words(text), 10)
                labelled = ask(label_form(text, names), 10)
                safe = ask(syntax_safe(text), 10)
                srv = ask(server_keywords(text), 10)
                safe_nonames = ask(syntax_safe(strip_names(text, names)), 10)
                rows.append({
                    "id": q["id"], "category": q["category"], "relevant": q["relevant"],
                    "variants": {
                        "base_n10": base,
                        "request_n3": ask(text, 3),
                        "request_n5": ask(text, 5),
                        "request_n50": ask(text, 50),
                        "query_without_speaker_names": nonames,
                        "query_content_words_only": keywords,
                        "query_as_stored_form": labelled,
                        "query_punctuation_stripped": safe,
                        "query_server_keywords_only": srv,
                        "rrf_base_plus_server_keywords": rrf([base, srv]),
                        "query_punctuation_and_names_stripped": safe_nonames,
                        "rrf_base_plus_punctuation_stripped": rrf([base, safe]),
                        "rrf_base_plus_without_names": rrf([base, nonames]),
                        "rrf_base_plus_content_words": rrf([base, keywords]),
                    },
                })

    def recall(row, name, k):
        return metrics.any_hit_at_k(row["variants"][name][:k], row["relevant"], k)

    names = list(rows[0]["variants"])
    summary = {}
    for scope, subset in [("all", rows)] + [
        (c, [r for r in rows if r["category"] == c])
        for c in sorted({r["category"] for r in rows})
    ]:
        summary[scope] = {"n": len(subset)}
        for name in names:
            summary[scope][name] = {
                f"any_recall@{k}": round(sum(recall(r, name, k) for r in subset) / len(subset), 4)
                for k in (1, 3, 5, 10)
                if k <= max(len(r["variants"][name]) for r in subset)
            }
            summary[scope][name]["any_recall@10_wins_vs_base"] = sum(
                1 for r in subset if recall(r, name, 10) > recall(r, "base_n10", 10)
            ) if name != "request_n3" and name != "request_n5" else None
            summary[scope][name]["any_recall@10_losses_vs_base"] = sum(
                1 for r in subset if recall(r, name, 10) < recall(r, "base_n10", 10)
            ) if name != "request_n3" and name != "request_n5" else None
    out = {
        "version": version, "dataset": provenance, "conversations": wanted,
        "date_in_text": not args.no_date, "summary": summary, "per_question": rows,
        "note": "request_n3/request_n5 are separate requests with that n_results (the product default is 3); "
                "only their @1/@3/@5 are meaningful. request_n50 is evaluated on its first 10 results.",
    }
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(out, handle, ensure_ascii=False, indent=1)
    for scope in ("all",):
        for name in names:
            print(scope, name, summary[scope][name])
    print("wrote", args.out, file=sys.stderr)


if __name__ == "__main__":
    main()
