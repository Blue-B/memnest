# Design decisions

<!-- markdownlint-disable MD013 -->

Why memnest is built the way it is. This document covers the architecture that ships, not experiments that remain outside the request path.

## Embeddings run locally

The store holds decisions, corrections, and conversation history. Sending every memory and query to a hosted embedding API would contradict the local-first boundary.

The Rust engine uses `fastembed` to run ONNX models in-process without a Python sidecar. The first operation that needs embeddings downloads the selected model. `memnest --warmup-embedding` lets an operator do that before the service starts.

## multilingual-e5-base is the default

A memory store can mix languages within one project, so the default must be multilingual. Prompt-time recall also has a short latency budget and runs beside an editor, browser, and coding agent. `intfloat/multilingual-e5-base` is the middle choice between model size, vector size, and multilingual retrieval quality.

The model remains configurable:

```bash
MEMNEST_EMBED_MODEL=BAAI/bge-m3
MEMNEST_EMBED_DIM=1024
```

Existing vectors use the previous dimension, so changing the model requires rebuilding the vector index.

## Keyword and vector search stay separate

BM25 finds exact tokens such as a port number or crate name but can miss a paraphrase. Vector similarity finds paraphrases but can drift past the literal text. The query decides which behavior matters, and the query is unknown when a memory is written, so each memory is indexed both ways.

The two searches return incomparable scores. Reciprocal Rank Fusion combines their positions instead of adding a BM25 score to a vector distance. MMR runs after fusion so near-duplicate memories do not fill the result set.

Neither search method proves that the store contains an answer. Callers must treat returned memories as candidates to verify.

## SQLite is the source of truth

SQLite holds every original record. Tantivy and HNSW hold derived indexes that can be rebuilt.

A memory row and its `index_queue` job are written in one transaction. The job is cleared only after both indexes are durable. A crash leaves enough information for startup to finish or rebuild the interrupted work.

Only one process may write a data directory. A second writer is rejected instead of racing SQLite and the indexes.

## Workspace scope uses a path hash

Memories are partitioned by working directory, but the public workspace ID is a stable hash of the normalized absolute path. This keeps two directories with the same basename separate without exposing the full local path.

An inferred search covers the current workspace plus `playbook`. A cross-project search requires `project=all`.

## Corrections preserve history

A caller replaces an outdated memory by saving the new value with `supersedes=<id>`. Both changes use one SQLite transaction and the old row moves to the hidden `_superseded` collection.

Memnest does not read source code and cannot decide that a saved statement became false. The caller that observes the change must write the correction.

## Secrets are separate from searchable memory

Ordinary memory text is stored unencrypted because the keyword and vector indexes must read it. Credential-shaped strings are redacted before storage, but pattern matching cannot catch every secret.

Credentials belong in the AES-256-GCM vault. Model-facing vault tools remain hidden unless `MEMNEST_EXPOSE_SECRET_TOOLS=1` is set for a trusted process.

## The prompt hook fails open

`memnest hook` runs while a user is sending a prompt. If the service is unavailable, the workspace is unknown, or another error occurs, it prints nothing and exits successfully. Memory recall must not stop someone from using their agent.

Retrieved content is labeled as untrusted reference data, and embedded markup is escaped before injection.

## Transcript capture uses one watcher

MCP exposes tool calls but not host conversation events. `memnest watch` therefore reads the transcript files already written by pi, Claude Code, and Codex.

The watcher stores visible user and assistant text and skips system prompts, reasoning, tool traffic, images, and subagent sidechains. One capture path is smaller and easier to audit than a separate extension for every host.
