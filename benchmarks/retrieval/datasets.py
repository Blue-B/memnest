"""Download public benchmark datasets into a cache outside the repository.

Only the standard library is used. Dataset contents are never written into the
repository; only the source URL, license and sha256 are recorded.
"""

import hashlib
import json
import os
import sys
import time
import urllib.request

CACHE_DIR = os.environ.get(
    "MEMNEST_BENCH_CACHE", os.path.expanduser("~/.cache/memnest-bench")
)

# LoCoMo is pinned to a commit so the bytes cannot change under us.
# LongMemEval-S "cleaned" is the file the upstream README currently points to
# (the 2025/09 cleaned release replaces the original longmemeval_s.json).
DATASETS = {
    "locomo": {
        "file": "locomo10.json",
        "url": "https://raw.githubusercontent.com/snap-research/locomo/"
        "3eb6f2c585f5e1699204e3c3bdf7adc5c28cb376/data/locomo10.json",
        "project": "https://github.com/snap-research/locomo",
        "license": "CC BY-NC 4.0 (snap-research/locomo LICENSE.txt); "
        "non-commercial: used here only to measure retrieval, not redistributed",
        "revision": "3eb6f2c585f5e1699204e3c3bdf7adc5c28cb376",
    },
    "longmemeval": {
        "file": "longmemeval_s_cleaned.json",
        "url": "https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/"
        "resolve/98d7416c24c778c2fee6e6f3006e7a073259d48f/longmemeval_s_cleaned.json",
        "project": "https://github.com/xiaowu0162/LongMemEval",
        "license": "MIT (Hugging Face dataset card xiaowu0162/longmemeval-cleaned; "
        "GitHub repository LICENSE)",
        "revision": "98d7416c24c778c2fee6e6f3006e7a073259d48f",
    },
}


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def fetch(name):
    """Return (local_path, provenance dict). Downloads once, then reuses the cache."""
    spec = DATASETS[name]
    os.makedirs(CACHE_DIR, exist_ok=True)
    path = os.path.join(CACHE_DIR, spec["file"])
    if not os.path.exists(path):
        tmp = path + ".part"
        if not spec["url"].startswith("https://"):
            raise ValueError(f"refusing non-https dataset URL: {spec['url']}")
        print(f"downloading {spec['url']}", file=sys.stderr)
        request = urllib.request.Request(
            spec["url"], headers={"User-Agent": "memnest-retrieval-bench"}
        )
        with urllib.request.urlopen(request, timeout=120) as response, open(
            tmp, "wb"
        ) as out:
            while True:
                block = response.read(1 << 20)
                if not block:
                    break
                out.write(block)
        os.replace(tmp, path)
    provenance = {
        "name": name,
        "file": spec["file"],
        "url": spec["url"],
        "project": spec["project"],
        "revision": spec["revision"],
        "license": spec["license"],
        "sha256": sha256_file(path),
        "bytes": os.path.getsize(path),
        "downloaded_at_cache_mtime": time.strftime(
            "%Y-%m-%dT%H:%M:%SZ", time.gmtime(os.path.getmtime(path))
        ),
    }
    return path, provenance


def load(name):
    path, provenance = fetch(name)
    with open(path, encoding="utf-8") as handle:
        return json.load(handle), provenance


if __name__ == "__main__":
    for dataset in sys.argv[1:] or DATASETS:
        _, info = fetch(dataset)
        print(json.dumps(info, indent=2))
