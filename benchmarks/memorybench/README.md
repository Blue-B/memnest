# MemoryBench integration

This directory contains a zero-SDK `MemnestProvider` matching MemoryBench's current `Provider` interface, plus a small Korean coding-memory benchmark fixture. The integration is pinned to:

- MemoryBench commit `94e2af54b661d90e77dddbd8fa4fa5b28c07a24e`
- MemoryBench package version `1.0.0`
- Bun `1.3.14` (the version used to validate this integration)
- Memnest HTTP endpoints `/health`, `/add`, `/search`, and `/prune` in this repository

No benchmark score is checked in. The local provider itself needs no paid key.

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

`fixtures/korean-coding-memory.json` has three Korean coding sessions and three questions covering fact recall and an updated port decision. `KoreanCodingBenchmark` implements the current MemoryBench `Benchmark` interface. Its deterministic tests validate schema, IDs, question filtering, evidence links, and that every required answer term occurs in both the designated evidence and ground truth. They do **not** turn those checks into a retrieval or answer score.

To use this fixture in a full MemoryBench run, copy `korean-coding-benchmark.ts` and the fixture into a benchmark module in the pinned checkout and register the module using MemoryBench's `src/benchmarks/README.md` instructions. This repository does not patch the fixture into upstream's CLI because upstream's closed `BenchmarkName` registry is independent of the Memnest provider integration.

## External-key and reproducibility blockers

- Adapter and fixture contract tests require no API keys and call no paid APIs.
- A full MemoryBench answer/evaluation run requires at least one key accepted by its answer model and judge (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `GOOGLE_API_KEY`). Without one, ingestion/search can be exercised but no official end-to-end score can be produced.
- Existing upstream benchmark datasets may be downloaded from GitHub or Hugging Face and therefore require network access.
- Memnest's first startup may require network access to download its local embedding model; later runs use the local cache.
- Retrieval ranking depends on the Memnest binary/model/configuration under test. Record the Memnest commit, model, MemoryBench run ID, selected questions, and all CLI flags when producing results.
