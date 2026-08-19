// Thin HTTP client for the memnest engine. All network access goes through an
// injectable `fetchFn` so the orchestration logic can be tested with a fake.

import type { Importance, MemoryCategory, SearchItem } from "./types.js";

export type FetchLike = (
  url: string,
  init?: {
    method?: string;
    headers?: Record<string, string>;
    body?: string;
    signal?: AbortSignal;
  },
) => Promise<{ ok: boolean; status: number; json: () => Promise<any>; text: () => Promise<string> }>;

export interface AddInput {
  text: string;
  project?: string;
  category?: MemoryCategory;
  importance?: Importance;
  chunkType?: "auto_log" | "manual" | "filtered" | "consolidated";
  sensitive?: boolean;
}

export interface NeighborItem {
  id: string;
  project: string;
  document: string;
  distance: number;
  // NOTE: the engine returns category/importance/chunk_type in PascalCase
  // (Rust `format!("{:?}", enum)`), e.g. "Failure"/"Knowledge"/"Manual", whereas
  // /add and /update accept snake_case. Always `.toLowerCase()` before comparing.
  category: string;
  importance: string;
  chunk_type: string;
}

export interface NeighborsOpts {
  id?: string;
  text?: string;
  k?: number;
  maxDistance?: number;
  project?: string;
}

export interface SearchOpts {
  project?: string;
  nResults?: number;
  recentFirst?: boolean;
}

export interface ContextOpts extends SearchOpts {
  maxNotes?: number;
  maxFacts?: number;
  maxChars?: number;
}

/** Project bucket where superseded chunks are parked (non-destructive). */
export const SUPERSEDED_PROJECT = "_superseded";

export class MemnestClient {
  constructor(
    private readonly baseUrl: string,
    private readonly fetchFn: FetchLike,
  ) {}

  private async get(path: string, signal?: AbortSignal): Promise<any> {
    const res = await this.fetchFn(`${this.baseUrl}${path}`, { method: "GET", signal });
    if (!res.ok) {
      throw new Error(`memnest ${path} -> HTTP ${res.status}: ${await safeText(res)}`);
    }
    return res.json();
  }

  private async post(path: string, body: unknown, signal?: AbortSignal): Promise<any> {
    const res = await this.fetchFn(`${this.baseUrl}${path}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
      signal,
    });
    if (!res.ok) {
      throw new Error(`memnest ${path} -> HTTP ${res.status}: ${await safeText(res)}`);
    }
    return res.json();
  }

  /** Persist a memory. Engine de-dups near-identical inserts in the background. */
  async add(input: AddInput): Promise<{ status: string; id: string; project: string }> {
    return this.post("/add", {
      text: input.text,
      project: input.project ?? "default",
      metadata: {
        chunk_type: input.chunkType ?? "manual",
        importance: input.importance ?? "knowledge",
        category: input.category ?? "general",
        sensitive: input.sensitive ?? false,
      },
    });
  }

  /**
   * Cosine nearest-neighbours from the engine's HNSW index. The robust
   * primitive for duplicate detection: catches paraphrase duplicates that
   * client-side lexical (trigram) similarity misses.
   */
  async neighbors(opts: NeighborsOpts): Promise<NeighborItem[]> {
    const out = await this.post("/neighbors", {
      id: opts.id ?? "",
      text: opts.text ?? "",
      k: opts.k ?? 10,
      max_distance: opts.maxDistance ?? 0,
      project: opts.project ?? "all",
    });
    return (out ?? []) as NeighborItem[];
  }

  async search(query: string, opts: SearchOpts = {}): Promise<SearchItem[]> {
    const body: any = {
      query,
      project: opts.project ?? "all",
      n_results: opts.nResults ?? 10,
      recent_first: opts.recentFirst ?? false,
    };
    if ((opts as any).category) body.category = (opts as any).category;
    const out = await this.post("/search", body);
    return (out?.results ?? []) as SearchItem[];
  }

  /**
   * Every chunk in one bucket, newest first, with no query involved.
   *
   * `/search` and `/neighbors` both start from a query embedding, so they can
   * only answer "what looks like this string". A standing injection block has no
   * such string, and feeding them a placeholder ranks by similarity to that
   * placeholder rather than by anything meaningful. This reads the bucket
   * directly instead, which is what makes durability ranking possible.
   *
   * The engine caps the response at 100 rows server-side (`get_chunks_by_project`
   * is a `ORDER BY created_at DESC LIMIT 100`), which is ample for the curated
   * buckets this is used on (`playbook`, `_user_model`) and bounds the payload
   * on large ones.
   */
  async collection(name: string): Promise<SearchItem[]> {
    const out = await this.get(`/collection/${encodeURIComponent(name)}`);
    return (Array.isArray(out) ? out : []) as SearchItem[];
  }

  /** Budget-bounded prompt pack (notes + facts + retrieved memories). */
  async context(query: string, opts: ContextOpts = {}): Promise<{ prompt: string }> {
    const out = await this.post("/context", {
      query,
      project: opts.project ?? "all",
      n_results: opts.nResults ?? 6,
      max_notes: opts.maxNotes ?? 12,
      max_facts: opts.maxFacts ?? 8,
      max_chars: opts.maxChars ?? 6000,
    });
    return { prompt: String(out?.prompt ?? "") };
  }

  async update(
    id: string,
    fields: { text?: string; project?: string; importance?: Importance; chunkType?: string },
  ): Promise<unknown> {
    const body: Record<string, unknown> = { id };
    if (fields.text !== undefined) body.text = fields.text;
    if (fields.project !== undefined) body.project = fields.project;
    if (fields.importance !== undefined) body.importance = fields.importance;
    if (fields.chunkType !== undefined) body.chunk_type = fields.chunkType;
    return this.post("/update", body);
  }

  /**
   * Non-destructively retire a chunk: re-parent it into the `_superseded`
   * bucket so it leaves normal project/all searches but stays recoverable
   * (and auditable) rather than being hard-deleted.
   */
  async supersede(id: string): Promise<unknown> {
    return this.update(id, { project: SUPERSEDED_PROJECT, chunkType: "consolidated" });
  }

  async summary(project: string, sessionId: string, summary: string): Promise<unknown> {
    return this.post("/summary", { project, session_id: sessionId, summary });
  }
}

async function safeText(res: { text: () => Promise<string> }): Promise<string> {
  try {
    return (await res.text()).slice(0, 200);
  } catch {
    return "<no body>";
  }
}
