# MemoryBench integration

This directory contains a zero-SDK `MemnestProvider` matching MemoryBench's current `Provider` interface, plus a small Korean coding-memory benchmark fixture. The integration is pinned to:

- MemoryBench commit `94e2af54b661d90e77dddbd8fa4fa5b28c07a24e`
- MemoryBench package version `1.0.0`
- Bun `1.3.14` (the version used to validate this integration)
- Memnest HTTP endpoints `/health`, `/add`, `/search`, and `/prune` in this repository

No comparative or answer-quality score is checked in. The local provider itself needs no paid key; the checked-in retrieval-only smoke result is labeled separately below.

## Contract validation (no keys and no daemon)

From the Memnest repository root:

```sh
bun test benchmarks/memorybench/test
```

The tests use an in-process fake HTTP transport. They validate request/response mapping, authentication behavior, synchronous indexing progress, cleanup, Korean text preservation, the MemoryBench benchmark views, and fixture references/ground truth.

## Reproduce against pinned MemoryBench

Clone exactly the reviewed upstream revision and install its locked dependencies:

```sh
git clone https://github.com/supermemoryai/memorybench.git /tmp/memorybench
cd /tmp/memorybench
git checkout 94e2af54b661d90e77dddbd8fa4fa5b28c07a24e
bun install --frozen-lockfile
node /path/to/memnest/benchmarks/memorybench/install-into-memorybench.mjs /tmp/memorybench
bunx tsc --noEmit
bun test
```

The installer only copies/registers `memnest`, reads `MEMNEST_URL`/`MEMNEST_TOKEN`, and extends the upstream provider-name union. It refuses any checkout not at the pinned commit and fails when a reviewed integration seam has drifted.

Start an isolated local Memnest daemon in another terminal (the first run may download the free local embedding model):

```sh
cd /path/to/memnest
rm -rf /tmp/memnest-memorybench-data
cargo run --manifest-path core/Cargo.toml --release -- \
  --data-dir /tmp/memnest-memorybench-data --host 127.0.0.1 --port 3111
```

Then, from `/tmp/memorybench`, configure the provider:

```sh
export MEMNEST_URL=http://127.0.0.1:3111
export MEMNEST_TOKEN=none
```

Use `MEMNEST_TOKEN=<the daemon token>` instead when daemon authentication is enabled. MemoryBench's CLI can now select `memnest` exactly as it selects its built-in providers. Container tags become isolated Memnest projects; `clear()` removes that project's unpinned benchmark memories through `/prune`.

## Korean coding-memory fixture

`fixtures/korean-coding-memory.json` has three Korean coding sessions and three questions covering fact recall and an updated port decision. `KoreanCodingBenchmark` implements the current MemoryBench `Benchmark` interface. Its deterministic tests validate schema, IDs, question filtering, evidence links, and that every required answer term occurs in both the designated evidence and ground truth. They do **not** turn those checks into an answer score.

Run the fixture against a disposable Memnest service without an answering or judge model:

```sh
cd /path/to/memnest
DATA=$(mktemp -d)
core/target/release/memnest --data-dir "$DATA" --port 3112 &
MEMNEST_PID=$!
MEMNEST_URL=http://127.0.0.1:3112 \
  bun benchmarks/memorybench/run-korean-retrieval.ts --out /tmp/memnest-ko.json
kill "$MEMNEST_PID"
```

The runner records Hit@1, Recall@3, MRR@10, successful-search rate, p50/p95 search latency, combined ingest-and-index wall time, disk bytes, and one end-of-run Linux RSS snapshot when `MEMNEST_PID` is supplied. Answer accuracy and context tokens remain `null` because reporting either without MemoryBench's answering phase would mislabel retrieval evidence. It clears its benchmark collection after recording the result.

[`results/2026-09-09-local-retrieval.json`](results/2026-09-09-local-retrieval.json) is the raw output of one Linux x64 run. All three tiny fixture questions ranked their expected session first; p50/p95 search latency was 24.8/27.4 ms, combined ingest-and-index time was 31.6 seconds, the end-of-run server RSS snapshot was 1.71 GB, and the indexed files used 22 KB. This is a smoke baseline, not a comparison or a claim about LongMemEval/LoCoMo accuracy.

To use this fixture in a full MemoryBench run, copy `korean-coding-benchmark.ts` and the fixture into a benchmark module in the pinned checkout and register the module using MemoryBench's `src/benchmarks/README.md` instructions. This repository does not patch the fixture into upstream's CLI because upstream's closed `BenchmarkName` registry is independent of the Memnest provider integration.

## Fair comparison command and blockers

After installing the adapter, a hosted Supermemory comparison would use one MemoryBench process so both providers share the same dataset slice, answering model, judge model, and pipeline settings:

```sh
bun run src/index.ts compare -p memnest,supermemory,filesystem,rag -b locomo \
  -m gpt-4o -j gpt-4o
bun run src/index.ts compare -p memnest,supermemory,filesystem,rag -b longmemeval \
  -m gpt-4o -j gpt-4o
```

Use full datasets rather than unseeded random sampling, record the generated compare and run IDs, and retain `data/runs/<run-id>/report.json`, checkpoints, and per-question search results. Do not compare separate runs with different model aliases, limits, or temperatures. The pinned MemoryBench model registry supplies the answering and judge settings; both providers in one `compare` invocation use those same settings.

- Adapter and fixture contract tests require no API keys and call no paid APIs.
- A full MemoryBench answer/evaluation run requires at least one key accepted by its answer model and judge (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `GOOGLE_API_KEY`). The current environment has no approved key or cost authorization, so the commands above were not run and no official end-to-end score exists.
- Hosted Supermemory additionally requires `SUPERMEMORY_API_KEY`. The `filesystem` baseline uses an OpenAI extraction call, and `rag` uses OpenAI extraction and embeddings, so those rows also incur external cost. The pinned upstream `supermemory` provider is hosted-only and exposes no equivalent local/self-hosted provider adapter, so a “Supermemory local” row cannot be produced from this reviewed revision without defining a different product and interface.
- Existing upstream benchmark datasets may be downloaded from GitHub or Hugging Face and therefore require network access.
- Memnest's first startup may require network access to download its local embedding model; later runs use the local cache.
- Retrieval ranking depends on the Memnest binary/model/configuration under test. Record the Memnest commit, model, MemoryBench run ID, selected questions, and all CLI flags when producing results.
