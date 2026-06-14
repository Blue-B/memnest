// Outcome reinforcement — the closed-loop "get better every task" engine.
//
// memnest-learn already *captures* memories; this module adds the missing
// feedback half: when the user signals an OUTCOME ("still broken" / "works
// now"), we find the memory that was about that pain and adjust its standing so
// the NEXT session ranks it differently.
//
//   recurrence  -> the same failure happened again: raise the matching
//                  failure/correction memory's importance and stamp a
//                  `[recurred ×N]` counter so it surfaces more prominently.
//   success     -> a memory we surfaced apparently helped: mark it validated
//                  (one importance step up, capped).
//
// All matching is done through the engine's cosine `/neighbors` (robust to
// paraphrase, unlike lexical match); the engine stays LLM-free and is only
// touched via the existing /neighbors + /update endpoints.

import type { MemnestClient } from "./memnest-client.js";
import type { Importance } from "./types.js";

export type OutcomeSignal = "recurrence" | "success" | null;

// ── importance ladder ────────────────────────────────────────────────────────
const LADDER: readonly Importance[] = ["log", "knowledge", "decision", "preference"];

/** Move one rung up the importance ladder, never past `cap`. */
export function bumpImportance(cur: string, cap: Importance = "preference"): Importance {
  const i = LADDER.indexOf(cur.toLowerCase() as Importance);
  const capIdx = LADDER.indexOf(cap);
  if (i < 0) return "knowledge";
  return LADDER[Math.min(i + 1, capIdx)]!;
}

// ── recurrence marker (audit trail of how often a failure repeated) ──────────
const MARKER_RE = /\s*\[recurred [×x](\d+)\]\s*$/i;

export function recurrenceCount(text: string): number {
  const m = MARKER_RE.exec(text);
  return m ? Number(m[1]) : 0;
}

export function withRecurrenceMarker(text: string, n: number): string {
  return `${text.replace(MARKER_RE, "").trimEnd()} [recurred ×${n}]`;
}

// ── outcome detection (EN + KO, ASCII-\b-free for Hangul) ────────────────────
const RECURRENCE_PATTERNS: RegExp[] = [
  /still\b[^.!?]*\b(broken|not working|fails?|failing|happening|the same|dying|crashing|crashes|down|there|here|wrong|bad)/i,
  /same (error|issue|problem|bug|thing)( again)?/i,
  /(did|does|doesn'?t|didn'?t) ?n'?t? (work|fix)/i,
  /not fixed/i,
  /\b(is|it'?s|it is|are|they'?re|thing'?s|that'?s) back\b/i,
  /\bback again\b/i,
  /\b(happening|crashing|dying|failing|broke|broken|error) again\b/i,
  /keeps? (happening|coming back|dying|crashing|failing|breaking)/i,
  /여전히/,
  /아직(도| 안|$)/,
  /또 (안|같은|터|죽|에러|발생|그)/,
  /다시[^.!?]*(죽|안|에러|터|발생)/,
  /재발/,
  /안 ?(고쳐|됐|돼|되|풀렸)/,
  /그대로(네|야|다|임|\s|$)/,
  /똑같(이|은|네|아)/,
];

const SUCCESS_PATTERNS: RegExp[] = [
  /(works|working|fine) now/i,
  /(that|it) (works|worked|fixed it|did it)/i,
  /\bfixed( it)?\b/i,
  /\bperfect\b/i,
  /됐(어|다|네|음)/,
  /(잘|이제) ?(돼|된다|됨|작동)/,
  /고쳐졌/,
  /해결(됐|했|돼|된)/,
  /성공(했|이|\s|$)/,
];

/** Classify a user turn as an outcome signal (recurrence wins over success). */
export function detectOutcomeSignal(text: string): OutcomeSignal {
  const t = text.trim();
  if (!t) return null;
  if (RECURRENCE_PATTERNS.some((re) => re.test(t))) return "recurrence";
  if (SUCCESS_PATTERNS.some((re) => re.test(t))) return "success";
  return null;
}

// ── reinforcement action ─────────────────────────────────────────────────────
export interface ReinforceResult {
  matched: boolean;
  id?: string;
  action?: "reinforced" | "validated";
  newImportance?: Importance;
  recurred?: number;
}

export interface ReinforceOpts {
  project?: string; // search scope (default "all")
  k?: number;
  maxDistance?: number; // cosine cap — must genuinely be the same memory
}

const RECUR_CATS = new Set(["failure", "correction", "tool_quirk"]);
const SUCCESS_CATS = new Set(["failure", "correction", "insight", "tool_quirk", "convention"]);

/**
 * Given an outcome signal and the text that described it, find the closest
 * relevant memory via cosine neighbours and reinforce it. Best-effort and
 * non-destructive: only `/update` (importance + recurrence marker), never
 * deletes. Returns what was changed (or `matched:false` if nothing close).
 */
export async function reinforce(
  client: MemnestClient,
  signal: OutcomeSignal,
  contextText: string,
  opts: ReinforceOpts = {},
): Promise<ReinforceResult> {
  if (!signal) return { matched: false };
  const text = contextText.trim();
  if (!text) return { matched: false };

  const maxDistance = opts.maxDistance ?? (signal === "recurrence" ? 0.22 : 0.2);
  const ns = await client.neighbors({
    text,
    k: opts.k ?? 8,
    maxDistance,
    project: opts.project ?? "all",
  });
  if (ns.length === 0) return { matched: false };

  const want = signal === "recurrence" ? RECUR_CATS : SUCCESS_CATS;
  const hit = ns.find((n) => want.has((n.category || "").toLowerCase())); // neighbours are distance-sorted
  if (!hit) return { matched: false };

  if (signal === "recurrence") {
    const n = recurrenceCount(hit.document) + 1;
    const newImportance = bumpImportance(hit.importance, "decision");
    await client.update(hit.id, {
      text: withRecurrenceMarker(hit.document, n),
      importance: newImportance,
    });
    return { matched: true, id: hit.id, action: "reinforced", newImportance, recurred: n };
  }

  // success: validate (one step up, capped at decision) — only if it changes
  const newImportance = bumpImportance(hit.importance, "decision");
  if (newImportance !== (hit.importance || "").toLowerCase()) {
    await client.update(hit.id, { importance: newImportance });
  }
  return { matched: true, id: hit.id, action: "validated", newImportance };
}
