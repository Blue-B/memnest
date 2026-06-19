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
  "Classify whether the user's latest message is a TRUE correction of a coding",
  "assistant, then convert it into ONE durable lesson for FUTURE sessions.",
  "A TRUE correction means the user says the assistant was wrong, guessed, ignored",
  "an instruction, missed visible evidence, repeated a known failure, or should",
  "have verified something instead of asserting it.",
  "Output NONE for ordinary questions, task requests, product/planning discussion,",
  "A-not-B preference changes, neutral skepticism, or meta-discussion about the",
  "memory/autolog system unless the message clearly corrects assistant behavior.",
  "If it IS a true correction, output ONLY the lesson — one self-contained sentence,",
  "no quotes, no prose, no markdown. Write it in the SAME language the user used.",
  "It must state BOTH what the assistant did wrong AND what to do instead, inferred",
  "from the conversation (e.g. 'don't assume the network is WiFi; verify with",
  "Get-NetAdapter'). Make it a concrete rule, not a restatement of the complaint.",
  "If you cannot infer a concrete lesson, output exactly: NONE",
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
  /** The text actually stored — usually the distilled lesson; raw only for high-confidence fallback. */
  lesson: string;
  /** true when an LLM distilled an actionable lesson; false for high-confidence raw fallback. */
  distilled: boolean;
}

/**
 * Correction fast-path: cheap regexes only select CANDIDATES. The final decision
 * is semantic: with an LLM + recent context, store only when the classifier can
 * infer an actionable future lesson. This prevents everyday Korean particles
 * ("잖아", "말고", "아니") from becoming durable correction memories.
 *
 * If the LLM is unavailable (missing or throws), store raw text only for very
 * high-confidence correction signals (e.g. "틀렸", "잘못", "추정", "내말무시").
 * If the LLM explicitly returns NONE, trust that semantic judgment and store nothing.
 */
export async function captureCorrection(
  correctionText: string,
  client: MemnestClient,
  project = "default",
  opts: { llm?: LlmComplete | null; context?: TranscriptTurn[] } = {},
): Promise<CorrectionCapture | null> {
  const raw = correctionText.trim();
  if (!raw) return null;

  let lesson: string | null = null;
  let distilled = false;
  const highConfidence = looksLikeDefiniteCorrection(raw);
  const hasSemanticJudge = !!(opts.llm && opts.context && opts.context.length > 0);
  let semanticJudgeUnavailable = !hasSemanticJudge;

  if (hasSemanticJudge) {
    try {
      const extracted = await extractCorrectionLesson(raw, opts.context!, opts.llm!);
      if (extracted) {
        lesson = extracted;
        distilled = true;
      }
    } catch {
      semanticJudgeUnavailable = true;
    }
  }

  // If the semantic judge explicitly returned NONE/null, respect it and do NOT
  // store raw text. Raw fallback is only for no judge or judge failure.
  if (!lesson && semanticJudgeUnavailable && highConfidence) lesson = raw;
  if (!lesson) return null;

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

/** High-confidence signals that are safe to store raw when semantic judging is unavailable. */
const DEFINITE_CORRECTION_PATTERNS: RegExp[] = [
  /\bno,? (use|don't|do not|that's wrong|not)\b/i,
  /\bthat'?s (wrong|incorrect|not right)\b/i,
  /\b(use|prefer) .* not\b/i,
  /틀렸/, // "wrong"
  /잘못/, // "mistaken/wrong"
  /추정|넘겨짚|단정|사실처럼/, // guessed / asserted as fact
  /확인(을)?\s*(했어야|해야|하고)|직접\s*확인/, // should have verified
  /내\s*말\s*무시|말\s*무시/, // ignored the user's words
  /(말했|하라\s*했|보라\s*했|확인하라\s*했|했)잖아/, // explicit "I told you / you did"
  /또\s*(실패|안돼|망|똑같)/, // repeating a past failure
  /의미\s*없/, // pointless / not useful
];

/**
 * Lower-confidence candidate signals. These only decide whether to ask the
 * semantic classifier. They are NOT stored raw on fallback.
 */
const CANDIDATE_CORRECTION_PATTERNS: RegExp[] = [
  ...DEFINITE_CORRECTION_PATTERNS,
  /\bactually,?\b/i,
  /^아니(야|요|라|라고)?(\s|[.!?]|$)/, // direct "no / that's not it", not mid-sentence "...아니야?"
  /그런\s*(거|것)\s*없|존재하지\s*않|없는\s*(파일|함수|메서드|옵션|설정|경로|API)/,
  /(해야|말아|하면).*(잖|는데)/, // "해야 했잖아"-style reproach, semantic judge filters ambiguity
  /그럴\s*(것|거|꺼)?\s*같/, // "이번에도 그럴 것 같아" = likely repeat of a prior bad pattern
  /(안 ?될|안 ?돼)\s*(것|거|꺼)?\s*같/, // failure prediction
];

export function looksLikeDefiniteCorrection(userText: string): boolean {
  const t = userText.trim();
  if (!t) return false;
  return DEFINITE_CORRECTION_PATTERNS.some((re) => re.test(t));
}

/** Candidate heuristic: true means "worth semantic judging", not "store it". */
export function looksLikeCorrection(userText: string): boolean {
  const t = userText.trim();
  if (!t) return false;
  return CANDIDATE_CORRECTION_PATTERNS.some((re) => re.test(t));
}
