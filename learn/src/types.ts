// Shared types for the memnest-learn layer. Deliberately framework-free so the
// pure logic modules can be unit-tested without pi or network access.

/** Semantic categories — mirror the Rust `MemoryCategory` (serde snake_case). */
export type MemoryCategory =
  | "general"
  | "failure"
  | "correction"
  | "insight"
  | "preference"
  | "convention"
  | "tool_quirk";

export const MEMORY_CATEGORIES: readonly MemoryCategory[] = [
  "general",
  "failure",
  "correction",
  "insight",
  "preference",
  "convention",
  "tool_quirk",
] as const;

/** Importance — mirrors the Rust `Importance` (serde snake_case). */
export type Importance = "log" | "knowledge" | "decision" | "preference";

/** A durable memory the learning layer extracted from a conversation. */
export interface LearnedMemory {
  category: MemoryCategory;
  /** One self-contained sentence/paragraph — what to remember. */
  text: string;
  /** Optional importance hint; defaults applied by category if omitted. */
  importance?: Importance;
}

/**
 * A memory row as the engine renders it. Returned by `POST /search` and, in the
 * same shape, by `GET /collection/{name}`.
 *
 * `helpful_count` / `harmful_count` carry recall feedback and are what let a
 * prompt-independent block rank by durability instead of by query similarity.
 * They are optional here because the fake clients in the test suite predate
 * them; treat a missing count as zero.
 */
export interface SearchItem {
  id: string;
  project: string;
  document: string;
  score: number;
  timestamp: string;
  chunk_type: string;
  importance: string;
  category: string;
  helpful_count?: number;
  harmful_count?: number;
}

/** Minimal transcript turn the capture step reasons over. */
export interface TranscriptTurn {
  role: "user" | "assistant" | "tool" | "system";
  text: string;
}

/**
 * The host-LLM bridge. The index wires this to pi's `complete(ctx.model, ...)`
 * (or a child `pi -p` process); tests pass a deterministic stub. This is the
 * ONLY place the layer touches an LLM — the memnest engine stays LLM-free.
 */
export type LlmComplete = (input: {
  system: string;
  user: string;
}) => Promise<string>;

/**
 * How durable each importance level is, high to low. Mirrors the routing rule
 * in `capture.ts`: `preference` and `decision` are the cross-project lessons
 * worth restating every session, `knowledge` is useful but situational, `log`
 * is chatter. The engine returns importance PascalCased (`"Preference"`), so
 * always lowercase before looking a value up here.
 */
const IMPORTANCE_RANK: Record<string, number> = {
  preference: 3,
  decision: 2,
  knowledge: 1,
  log: 0,
};

function importanceRank(importance: string): number {
  return IMPORTANCE_RANK[importance.trim().toLowerCase()] ?? 0;
}

/**
 * Order memories for a prompt-INDEPENDENT block by how durable they are.
 *
 * The injection snapshot is standing context, not an answer to the current
 * prompt, so relevance to a query is the wrong signal. Ranking by cosine
 * distance to a fixed placeholder string (what this layer used to do) is not
 * relevance at all, it is noise with a threshold on top. Durability is what a
 * standing block actually wants: how important the memory was judged, whether
 * feedback confirmed it, and how recent it is.
 *
 * The comparator is a total order (id breaks ties last), so the same rows
 * always render the same bytes. That is what keeps the snapshot prefix-cache
 * stable. Only stored fields are compared, never wall-clock age, so the result
 * does not drift between turns.
 */
export function rankByDurability<T extends SearchItem>(items: readonly T[]): T[] {
  const net = (i: T) => (i.helpful_count ?? 0) - (i.harmful_count ?? 0);
  return [...items].sort(
    (a, b) =>
      importanceRank(b.importance) - importanceRank(a.importance) ||
      net(b) - net(a) ||
      b.timestamp.localeCompare(a.timestamp) ||
      a.id.localeCompare(b.id),
  );
}

/** Importance fallback per category when the extractor doesn't specify one. */
export function defaultImportanceFor(category: MemoryCategory): Importance {
  switch (category) {
    case "preference":
      return "preference";
    case "correction":
    case "convention":
      return "decision";
    case "failure":
    case "insight":
    case "tool_quirk":
      return "knowledge";
    default:
      return "log";
  }
}
