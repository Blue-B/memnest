# Memnest benchmarks

Benchmark integrations live below this directory and are intentionally isolated from the product runtime. `memorybench/` does not contain or publish scores; `retrieval/` does (see below).

- [`memorybench/`](memorybench/README.md): MemoryBench provider adapter and a deterministic Korean coding-memory fixture.
- [`retrieval/`](retrieval/run.py): retrieval-only harness for LoCoMo and LongMemEval-S (Python standard library only, no LLM). It starts a scratch memnest, ingests one memory per dialog turn, sends each question unchanged to `/search`, and writes recall / MRR / NDCG plus per-question hits to `retrieval/results/`. Results, scope and limits: [docs/benchmarks.md](../docs/benchmarks.md) ([한국어](../docs/benchmarks.ko.md)); miss analysis: [`retrieval/ANALYSIS.md`](retrieval/ANALYSIS.md). These are retrieval recall numbers, not end-to-end QA accuracy.

```sh
python3 benchmarks/retrieval/test_metrics.py
python3 benchmarks/retrieval/run.py --binary ~/.local/bin/memnest --dataset locomo
```

Datasets are downloaded to `~/.cache/memnest-bench` and are never committed.
