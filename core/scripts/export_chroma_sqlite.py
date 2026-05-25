#!/usr/bin/env python3
"""Export ChromaDB memories to JSONL without the chromadb Python package.

This reads Chroma's sqlite metadata store directly. Vector payloads are not
portable across all Chroma versions, so this exporter intentionally writes
``embedding: null`` and lets Palimpsest re-embed during import.
"""

import argparse
import json
import os
import sqlite3
import sys


def scalar_value(row):
    _, key, string_value, int_value, float_value, bool_value = row
    if string_value is not None:
        return string_value
    if int_value is not None:
        return int_value
    if float_value is not None:
        return float_value
    if bool_value is not None:
        return bool(bool_value)
    return None


def normalize_chunk_type(value):
    value = str(value or "auto_log").lower()
    if value in {"manual", "filtered", "consolidated", "auto_log"}:
        return value
    if value in {"session_end", "summary", "session_summary"}:
        return "consolidated"
    return "auto_log"


def normalize_importance(value):
    value = str(value or "log").lower()
    if value in {"log", "knowledge", "decision", "preference"}:
        return value
    return "knowledge" if value in {"important", "fact"} else "log"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--chroma", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    db_path = os.path.join(args.chroma, "chroma.sqlite3")
    if not os.path.exists(db_path):
        print(f"ChromaDB not found: {db_path}", file=sys.stderr)
        return 1

    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    cursor.execute(
        """
        SELECT e.id, e.embedding_id, e.created_at, c.name
        FROM embeddings e
        JOIN segments s ON e.segment_id = s.id
        JOIN collections c ON s.collection = c.id
        WHERE s.scope = 'METADATA'
        ORDER BY e.id
        """
    )
    rows = cursor.fetchall()

    metadata_by_id = {}
    cursor.execute("SELECT id, key, string_value, int_value, float_value, bool_value FROM embedding_metadata")
    for row in cursor.fetchall():
        item_id = row[0]
        key = row[1]
        metadata_by_id.setdefault(item_id, {})[key] = scalar_value(row)

    total = 0
    with open(args.out, "w", encoding="utf-8") as f:
        for numeric_id, embedding_id, created_at, col_name in rows:
            metadata = metadata_by_id.get(numeric_id, {})
            document = metadata.get("chroma:document") or metadata.get("document") or ""
            if not str(document).strip():
                continue

            project = metadata.get("project") or col_name.replace("droid_", "")
            record = {
                "id": embedding_id,
                "project": project,
                "document": document,
                "embedding": None,
                "metadata": {
                    "chunk_type": normalize_chunk_type(metadata.get("type")),
                    "importance": normalize_importance(metadata.get("importance")),
                    "session_id": metadata.get("session_id") or "",
                    "raw_chunk": metadata.get("raw_chunk"),
                    "access_count": int(metadata.get("access_count", 0) or 0),
                    "keywords": [x for x in str(metadata.get("keywords", "")).split(",") if x],
                    "source": "chroma_sqlite_import",
                },
                "created_at": created_at,
            }
            f.write(json.dumps(record, ensure_ascii=False) + "\n")
            total += 1

    print(f"exported {total} memories to {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
