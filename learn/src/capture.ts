// Capture: extract durable memories from a conversation slice and persist them
// to memnest with the right category + importance. The engine de-dups
// near-identical inserts in the background, so re-running capture is safe.

import { extractMemories } from "./extract.js";
import type { MemnestClient } from "./memnest-client.js";
import { defaultImportanceFor, type LearnedMemory, type LlmComplete, type TranscriptTurn } from "./types.js";

export interface CaptureResult {
  extracted: number;
  written: string[]; // ids returned by the engine
  errors: number;
  /** The (in-batch de-duped) memories actually persisted — for downstream
   *  routing into skill self-improvement / the user model. */
  memories: LearnedMemory[];
}

export interface CaptureOpts {
  project?: string;
  /** Optional cap on how many memories to persist from one capture pass. */
  max?: number;
}

export async function captureMemories(
  turns: TranscriptTurn[],
  llm: LlmComplete,
  client: MemnestClient,
  opts: CaptureOpts = {},
): Promise<CaptureResult> {
  const memories = await extractMemories(turns, llm);
  const limited = opts.max ? memories.slice(0, opts.max) : memories;
  // Simple in-batch dedup (normalized text) so one LLM response doesn't spam identical entries
  const seen = new Set<string>();
  const written: string[] = [];
  const persisted: LearnedMemory[] = [];
  let errors = 0;
  for (const m of limited) {
    const key = m.text.trim().toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    try {
      const res = await client.add({
        text: m.text,
        project: opts.project ?? "default",
        category: m.category,
        importance: m.importance ?? defaultImportanceFor(m.category),
        chunkType: "manual",
      });
      if (res?.id) written.push(res.id);
      persisted.push(m);
    } catch {
      errors++;
    }
  }
  return { extracted: memories.length, written, errors, memories: persisted };
}

/**
 * Correction fast-path: when the user explicitly corrects the agent, store it
 * immediately as a high-signal `correction` memory without waiting for the
 * periodic capture pass.
 */
export async function captureCorrection(
  correctionText: string,
  client: MemnestClient,
  project = "default",
): Promise<string | null> {
  const text = correctionText.trim();
  if (!text) return null;
  const res = await client.add({
    text,
    project,
    category: "correction",
    importance: "decision",
    chunkType: "manual",
  });
  return res?.id ?? null;
}

/** Heuristic: does this user message look like a correction of the agent? */
const CORRECTION_PATTERNS: RegExp[] = [
  /\bno,? (use|don't|do not|that's wrong|not)\b/i,
  /\bactually,?\b/i,
  /\bthat'?s (wrong|incorrect|not right)\b/i,
  /\b(use|prefer) .* not\b/i,
  /아니(야|요|라|\s|$)/, // Korean: "no / that's not it" (\b is ASCII-only, useless for Hangul)
  /틀렸/, // Korean: "wrong"
  /잘못/, // Korean: "mistaken/wrong"
];

export function looksLikeCorrection(userText: string): boolean {
  const t = userText.trim();
  if (!t) return false;
  return CORRECTION_PATTERNS.some((re) => re.test(t));
}
