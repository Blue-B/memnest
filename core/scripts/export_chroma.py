#!/usr/bin/env python3
"""Export ChromaDB memories to JSONL for memnest Rust import.

Usage:
  python scripts/export_chroma.py \
    --chroma ~/.factory/memories/chroma_db \
    --out /tmp/memories.jsonl

Each line has: id, project, document, embedding, metadata.
"""

import argparse
import json
import os
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--chroma", default=os.path.expanduser("~/.factory/memories/chroma_db"))
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    try:
        import chromadb
    except ImportError:
        print("chromadb is required: pip install chromadb", file=sys.stderr)
        return 2

    client = chromadb.PersistentClient(path=os.path.expanduser(args.chroma))
    total = 0
    with open(args.out, "w", encoding="utf-8") as f:
        for col in client.list_collections():
            try:
                collection = client.get_collection(col.name)
                result = collection.get(include=["documents", "metadatas", "embeddings"])
            except Exception as e:
                print(f"skip {col.name}: {e}", file=sys.stderr)
                continue

            ids = result.get("ids") or []
            docs = result.get("documents") or []
            metas = result.get("metadatas") or []
            embeddings = result.get("embeddings") or []
            for i, item_id in enumerate(ids):
                meta = metas[i] if i < len(metas) and isinstance(metas[i], dict) else {}
                project = meta.get("project") or col.name.replace("droid_", "")
                record = {
                    "id": item_id,
                    "project": project,
                    "document": docs[i] if i < len(docs) else "",
                    "embedding": embeddings[i] if i < len(embeddings) else None,
                    "metadata": {
                        "chunk_type": meta.get("type", "auto_log"),
                        "importance": meta.get("importance", "log"),
                        "session_id": meta.get("session_id", ""),
                        "raw_chunk": meta.get("raw_chunk"),
                        "access_count": int(meta.get("access_count", 0) or 0),
                        "keywords": [x for x in str(meta.get("keywords", "")).split(",") if x],
                    },
                }
                f.write(json.dumps(record, ensure_ascii=False) + "\n")
                total += 1

    print(f"exported {total} memories to {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
