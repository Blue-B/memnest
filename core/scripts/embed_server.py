#!/usr/bin/env python3
"""Local embedding sidecar for palimpsest.

Default model is multilingual. If sentence-transformers is unavailable, the
server falls back to deterministic lexical vectors so development still works.

Usage:
  pip install sentence-transformers
  python scripts/embed_server.py --model paraphrase-multilingual-mpnet-base-v2 --port 9500
  PALIMPSEST_EMBED_URL=http://127.0.0.1:9500/embed palimpsest ...
"""

import argparse
import hashlib
import json
import math
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Embedder:
    def __init__(self, model_name: str, dim: int):
        self.model_name = model_name
        self.dim = dim
        self.model = None
        try:
            from sentence_transformers import SentenceTransformer
            self.model = SentenceTransformer(model_name)
            # Determine actual output dim lazily from the model.
            self.dim = int(self.model.get_sentence_embedding_dimension() or dim)
            print(f"[embed] loaded sentence-transformers model: {model_name} dim={self.dim}")
        except Exception as exc:
            print(f"[embed] sentence-transformers unavailable, using lexical fallback: {exc}")

    def encode(self, texts):
        if self.model is not None:
            vectors = self.model.encode(texts, normalize_embeddings=True, show_progress_bar=False)
            return [list(map(float, v)) for v in vectors]
        return [self._lexical(text) for text in texts]

    def _lexical(self, text: str):
        vector = [0.0] * self.dim
        for token in tokenize(text):
            digest = hashlib.sha256(token.encode("utf-8")).digest()
            idx = int.from_bytes(digest[:8], "little") % self.dim
            sign = 1.0 if digest[8] & 1 == 0 else -1.0
            weight = 1.0 + min(len(token), 12) / 12.0
            vector[idx] += sign * weight
        norm = math.sqrt(sum(x * x for x in vector))
        if norm > 0:
            vector = [x / norm for x in vector]
        return vector


def tokenize(text: str):
    out = []
    cur = []
    for ch in text:
        if ch.isalnum() or ch in "_-./":
            cur.append(ch.lower())
        elif cur:
            out.append("".join(cur))
            cur.clear()
    if cur:
        out.append("".join(cur))
    return out


def make_handler(embedder: Embedder):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path == "/health":
                self.send_json({"status": "ok", "model": embedder.model_name, "dim": embedder.dim})
            else:
                self.send_error(404)

        def do_POST(self):
            if self.path != "/embed":
                self.send_error(404)
                return
            length = int(self.headers.get("content-length", "0"))
            payload = json.loads(self.rfile.read(length).decode("utf-8"))
            texts = payload.get("texts") or []
            if not isinstance(texts, list):
                self.send_error(400, "texts must be a list")
                return
            self.send_json({"embeddings": embedder.encode([str(t) for t in texts])})

        def send_json(self, obj):
            raw = json.dumps(obj, ensure_ascii=False).encode("utf-8")
            self.send_response(200)
            self.send_header("content-type", "application/json; charset=utf-8")
            self.send_header("content-length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)

        def log_message(self, fmt, *args):
            return

    return Handler


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="paraphrase-multilingual-mpnet-base-v2")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=9500)
    parser.add_argument("--dim", type=int, default=768)
    args = parser.parse_args()

    embedder = Embedder(args.model, args.dim)
    server = ThreadingHTTPServer((args.host, args.port), make_handler(embedder))
    print(f"[embed] listening on http://{args.host}:{args.port}/embed")
    server.serve_forever()


if __name__ == "__main__":
    main()
