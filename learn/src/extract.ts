// Memory extraction: turn a slice of conversation into categorised durable
// memories. The prompt is built here; the LLM call is injected (host model).
// Parsing is pure and defensive so a chatty model can't crash the pipeline.

import {
  MEMORY_CATEGORIES,
  type LearnedMemory,
  type LlmComplete,
  type MemoryCategory,
  type TranscriptTurn,
} from "./types.js";

export const EXTRACTION_SYSTEM_PROMPT = [
  "You extract durable, reusable memories from a coding-assistant conversation.",
  "Return ONLY a JSON array (no prose, no code fence). Each element:",
  '{ "category": one of [' + MEMORY_CATEGORIES.join(", ") + '], "text": string, "importance"?: "log"|"knowledge"|"decision"|"preference" }',
  "Rules:",
  "- Save only things worth recalling in a FUTURE session: failures + their cause, user corrections, durable preferences, project conventions, tool quirks, hard-won insights.",
  "- Do NOT save transient chatter, restated file contents, or anything trivially re-derivable.",
  "- Each text must be ONE self-contained sentence, understandable with no other context.",
  "- Never include secrets/credentials.",
  "- If nothing is worth saving, return [].",
].join("\n");

export function buildExtractionUserPrompt(turns: TranscriptTurn[], maxChars = 12000): string {
  let text = turns
    .filter((t) => t.role !== "system")
    .map((t) => `${t.role.toUpperCase()}: ${t.text}`)
    .join("\n");
  if (text.length > maxChars) text = text.slice(-maxChars); // keep the recent tail
  return `Conversation:\n${text}\n\nReturn the JSON array of memories to save.`;
}

/** Extract the first top-level JSON array from a (possibly chatty) LLM reply. */
export function extractJsonArray(raw: string): unknown {
  const fenced = raw.replace(/```(?:json)?/gi, "");
  const start = fenced.indexOf("[");
  if (start === -1) return [];
  let depth = 0;
  for (let i = start; i < fenced.length; i++) {
    const ch = fenced[i];
    if (ch === "[") depth++;
    else if (ch === "]") {
      depth--;
      if (depth === 0) {
        try {
          return JSON.parse(fenced.slice(start, i + 1));
        } catch {
          return [];
        }
      }
    }
  }
  return [];
}

function isCategory(v: unknown): v is MemoryCategory {
  return typeof v === "string" && (MEMORY_CATEGORIES as readonly string[]).includes(v);
}

const IMPORTANCES = ["log", "knowledge", "decision", "preference"] as const;

/** Parse + validate the model output into clean LearnedMemory records. */
export function parseExtraction(raw: string): LearnedMemory[] {
  const arr = extractJsonArray(raw);
  if (!Array.isArray(arr)) return [];
  const out: LearnedMemory[] = [];
  for (const el of arr) {
    if (!el || typeof el !== "object") continue;
    const obj = el as Record<string, unknown>;
    const text = typeof obj.text === "string" ? obj.text.trim() : "";
    if (!text) continue;
    const category: MemoryCategory = isCategory(obj.category) ? obj.category : "general";
    const importance =
      typeof obj.importance === "string" && (IMPORTANCES as readonly string[]).includes(obj.importance)
        ? (obj.importance as LearnedMemory["importance"])
        : undefined;
    out.push({ category, text, importance });
  }
  return out;
}

/** End-to-end extraction: build prompt, call the host LLM, parse. */
export async function extractMemories(
  turns: TranscriptTurn[],
  llm: LlmComplete,
): Promise<LearnedMemory[]> {
  if (turns.length === 0) return [];
  const reply = await llm({
    system: EXTRACTION_SYSTEM_PROMPT,
    user: buildExtractionUserPrompt(turns),
  });
  return parseExtraction(reply);
}
