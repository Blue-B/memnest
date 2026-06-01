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

/** A search result returned by the memnest engine (`POST /search`). */
export interface SearchItem {
  id: string;
  project: string;
  document: string;
  score: number;
  timestamp: string;
  chunk_type: string;
  importance: string;
  category: string;
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
