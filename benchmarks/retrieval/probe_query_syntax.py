#!/usr/bin/env python3
"""Which punctuation in a query silently disables memnest's BM25 channel?

The text index parses the raw query with tantivy's QueryParser
(core/src/index/mod.rs search_projects) and the caller swallows parse errors
(`unwrap_or_default()` in core/src/server/api.rs run_hybrid_search_scope).
A one-keyword query has no vector-only fallback (needs >= 2 keywords), so if the
lexical channel finds nothing the result list is empty. (Queries with 2+ keywords
can still be rescued by the 3-slot vector-only fallback, so only the single-keyword
rows are a clean test; the LoCoMo results show the real-world effect.) This probe stores one memory that
contains the rare token and checks whether it is found.

    python3 benchmarks/retrieval/probe_query_syntax.py --binary ~/.local/bin/memnest
"""

import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import instance  # noqa: E402

RARE_WORD = "zebraquux"
CASES = [
    ("plain", f"{RARE_WORD}"),
    ("apostrophe 's", f"{RARE_WORD}'s"),
    ("possessive before", f"Caroline's {RARE_WORD}"),
    ("trailing question mark", f"{RARE_WORD}?"),
    ("comma", f"{RARE_WORD}, ok"),
    ("period", f"{RARE_WORD}."),
    ("exclamation", f"{RARE_WORD}!"),
    ("colon after word", f"error: {RARE_WORD}"),
    ("colon path:line", f"src/lib.rs:10 {RARE_WORD}"),
    ("double colon", f"std::{RARE_WORD}"),
    ("hyphen inside word", f"well-known {RARE_WORD}"),
    ("leading minus", f"-{RARE_WORD}"),
    ("flag --", f"--{RARE_WORD}"),
    ("plus", f"+{RARE_WORD}"),
    ("open paren", f"({RARE_WORD}"),
    ("paren pair", f"foo({RARE_WORD})"),
    ("unbalanced close paren", f"{RARE_WORD})"),
    ("square bracket", f"[{RARE_WORD}]"),
    ("curly brace", f"{{{RARE_WORD}}}"),
    ("one double quote", f'{RARE_WORD} "'),
    ("balanced double quotes", f'"{RARE_WORD}"'),
    ("tilde", f"{RARE_WORD}~"),
    ("caret", f"{RARE_WORD}^2"),
    ("asterisk", f"{RARE_WORD}*"),
    ("slash", f"a/b {RARE_WORD}"),
    ("backslash", f"a\\b {RARE_WORD}"),
    ("ampersand", f"a && {RARE_WORD}"),
    ("uppercase AND", f"{RARE_WORD} AND"),
    ("uppercase OR", f"{RARE_WORD} OR"),
    ("hash", f"#{RARE_WORD}"),
    ("at sign", f"@{RARE_WORD}"),
    ("dollar", f"${RARE_WORD}"),
    ("equals", f"a={RARE_WORD}"),
    ("semicolon", f"{RARE_WORD};"),
    ("angle brackets", f"<{RARE_WORD}>"),
    ("curly apostrophe", f"{RARE_WORD}’s"),
]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--out", default="")
    args = parser.parse_args()
    rows = []
    with instance.Instance(args.binary) as scratch:
        client = scratch.client
        if client is None:
            raise RuntimeError("no client")
        version = scratch.health()["version"]
        meta = {"chunk_type": "auto_log", "importance": "log", "session_id": "s",
                "source": "bench.transcript", "adapter": "bench", "event_id": "e1",
                "sequence": 1, "total": 1}
        client.post("/add", {"text": f"User said: the {RARE_WORD} is in the garage", "project": "probe", "metadata": meta})
        client.post("/add", {"text": "User said: unrelated note about lunch", "project": "probe",
                             "metadata": dict(meta, event_id="e2")})
        for name, query in CASES:
            status, body = client.post("/search", {"query": query, "project": "probe", "n_results": 5})
            results = body["results"] if status == 200 and isinstance(body, dict) else []
            found = any(RARE_WORD in r["document"] for r in results)
            rows.append({"case": name, "query": query, "lexical_channel_worked": found,
                         "results": len(results)})
    print(f"memnest {version}")
    for r in rows:
        print(f"{'ok  ' if r['lexical_channel_worked'] else 'FAIL'} {r['case']:<26} {r['query']!r}")
    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            json.dump({"version": version, "rare_word": RARE_WORD, "cases": rows}, handle, indent=1, ensure_ascii=False)


if __name__ == "__main__":
    main()
