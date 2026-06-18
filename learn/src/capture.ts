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

// Turn a raw in-the-moment complaint into a durable, self-contained lesson.
// The user's correction ("너 왜 또 추정해?") has no actionable content on its
// own; the real lesson lives in what the assistant just did. We hand the model
// the recent turns + the complaint and ask for ONE forward-looking rule.
export const CORRECTION_SYSTEM_PROMPT = [
  "You convert a user's in-the-moment correction of a coding assistant into ONE",
  "durable lesson for FUTURE sessions. The user's words are a complaint; the",
  "actual lesson lives in what the assistant just did wrong.",
  "Output ONLY the lesson — one self-contained sentence, no quotes, no prose,",
  "no markdown. Write it in the SAME language the user used.",
  "It must state BOTH what the assistant did wrong AND what to do instead,",
  "inferred from the conversation (e.g. 'don't assume the network is WiFi —",
  "verify with Get-NetAdapter'). Make it a concrete rule, not a restatement of",
  "the complaint. If you cannot infer a concrete lesson, output exactly: NONE",
].join("\n");

export async function extractCorrectionLesson(
  correctionText: string,
  context: TranscriptTurn[],
  llm: LlmComplete,
): Promise<string | null> {
  const convo = context
    .filter((t) => t.role !== "system")
    .slice(-8)
    .map((t) => `${t.role.toUpperCase()}: ${t.text.slice(0, 1500)}`)
    .join("\n");
  const reply = (
    await llm({
      system: CORRECTION_SYSTEM_PROMPT,
      user: `Conversation:\n${convo}\n\nThe user's correction: ${correctionText}\n\nReturn the one-sentence lesson.`,
    })
  )
    .trim()
    .replace(/^["'`]+|["'`]+$/g, "")
    .trim();
  if (!reply || reply === "NONE" || /^none$/i.test(reply)) return null;
  return reply.slice(0, 500);
}

export interface CorrectionCapture {
  id: string | null;
  /** The text actually stored — the distilled lesson, or the raw complaint on fallback. */
  lesson: string;
  /** true when an LLM distilled an actionable lesson; false when we stored the raw complaint. */
  distilled: boolean;
}

/**
 * Correction fast-path: when the user explicitly corrects the agent, store a
 * high-signal `correction` memory immediately without waiting for the periodic
 * capture pass. When an LLM + recent context are supplied, distil the complaint
 * into an actionable lesson first; otherwise fall back to storing the raw text.
 */
export async function captureCorrection(
  correctionText: string,
  client: MemnestClient,
  project = "default",
  opts: { llm?: LlmComplete | null; context?: TranscriptTurn[] } = {},
): Promise<CorrectionCapture | null> {
  const raw = correctionText.trim();
  if (!raw) return null;

  let lesson = raw;
  let distilled = false;
  if (opts.llm && opts.context && opts.context.length > 0) {
    try {
      const extracted = await extractCorrectionLesson(raw, opts.context, opts.llm);
      if (extracted) {
        lesson = extracted;
        distilled = true;
      }
    } catch {
      /* LLM unavailable — fall back to raw complaint */
    }
  }

  const res = await client.add({
    text: lesson,
    project,
    category: "correction",
    // a distilled rule is a durable preference; a raw complaint is a weaker signal
    importance: distilled ? "preference" : "decision",
    chunkType: "manual",
  });
  return { id: res?.id ?? null, lesson, distilled };
}

/**
 * Extract plain text from a message's content, which the runtime may give as a
 * string or an array of content blocks ({type:"text", text}). Non-text blocks
 * (tool calls, images, thinking) are skipped. Used by the agent_end hook so the
 * learning layer also sees what the ASSISTANT said, not just user turns.
 */
export function extractMessageText(content: unknown): string {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  const parts: string[] = [];
  for (const block of content) {
    if (typeof block === "string") parts.push(block);
    else if (
      block &&
      typeof block === "object" &&
      (block as any).type === "text" &&
      typeof (block as any).text === "string"
    ) {
      parts.push((block as any).text);
    }
  }
  return parts.join("\n").trim();
}

/** Heuristic: does this user message look like a correction of the agent? */
const CORRECTION_PATTERNS: RegExp[] = [
  /\bno,? (use|don't|do not|that's wrong|not)\b/i,
  /\bactually,?\b/i,
  /\bthat'?s (wrong|incorrect|not right)\b/i,
  /\b(use|prefer) .* not\b/i,
  // -- Korean correction patterns --
  /아니(야|요|라|\s|$)/, // "no / that's not it"
  /틀렸/, // "wrong"
  /잘못/, // "mistaken/wrong"
  /추정/, // "추정했잖아" = you assumed (guessed instead of verifying)
  /말고/, // "추정말고 직접확인해" = don't guess, verify directly
  /없는데/, // "F32그런거없는데?" = that doesn't exist
  /했잖아/, // "추정했잖아" = I already told you / you did it again
  /잖아(요)?/, // general "as you should know" or "as I said" correction tone
  /(해야|말아|하면).*(잖|는데)/, // "해야하잖아" = you should have… (correction)
  // -- Korean skepticism / failure-prediction. A doubt that the agent will
  // repeat a past mistake is itself a correction signal: it means "last time's
  // lesson is not being applied — raise its priority". Kept narrow so a positive
  // "~ㄹ 것 같아" (e.g. "맞는 것 같아") is NOT matched: a negative pointer
  // ("그럴" / "안 될" / "또 실패") must be present.
  /그럴\s*(것|거|꺼)?\s*같/, // "이번에도 그럴 것 같아" = it'll go the same (bad) way again
  /(안 ?될|안 ?돼)\s*(것|거|꺼)?\s*같/, // "이것도 안될것같아" = this won't work either
  /또\s*(실패|안돼|망|똑같)/, // "또 실패할 것 같아" = going to fail again
  /의미\s*없/, // "해도 의미없어" = pointless even if we do it
];

export function looksLikeCorrection(userText: string): boolean {
  const t = userText.trim();
  if (!t) return false;
  return CORRECTION_PATTERNS.some((re) => re.test(t));
}
