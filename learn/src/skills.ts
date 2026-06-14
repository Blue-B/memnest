// Skill self-improvement (Hermes' signature loop).
//
// The base layer's `skill` tool is create/find only — a skill, once written,
// never changes. Hermes' insight is that skills should *improve during use*.
// `improveSkills` runs after a capture pass: for each procedural learning it
//   - refines the nearest existing skill (append the new step/caveat), or
//   - drafts a brand-new skill when none is close and the learning is a genuine
//     reusable procedure.
//
// Non-destructive: existing skills are updated in place (no supersede); the LLM
// is the host model (injected), the engine stays LLM-free.

import type { MemnestClient } from "./memnest-client.js";
import type { LearnedMemory, LlmComplete, MemoryCategory } from "./types.js";

export const SKILL_PROJECT = "_skills";

/** Categories that can plausibly carry a reusable "how-to". */
const PROCEDURAL_CATS = new Set<MemoryCategory>([
  "convention",
  "insight",
  "tool_quirk",
  "correction",
]);

export function isSkillCandidate(m: LearnedMemory): boolean {
  return PROCEDURAL_CATS.has(m.category);
}

export const SKILL_REFINE_SYSTEM_PROMPT = [
  "You maintain a reusable skill (a how-to procedure) for a coding agent.",
  "Given the CURRENT skill and a NEW learning, return ONLY the improved skill text",
  "(no prose, no quotes, no code fence).",
  "Integrate the new learning as a step or caveat. Preserve every existing step.",
  "Keep it concise, ordered, and self-contained. Keep the leading '# Title' line.",
].join("\n");

export const SKILL_DRAFT_SYSTEM_PROMPT = [
  "You write a short reusable skill (how-to) for a coding agent from one learning.",
  "If the learning is a genuine reusable procedure, return:",
  "  a single '# Title' line, then 2-6 numbered steps.",
  "If it is NOT a reusable procedure (just a fact/preference), return exactly: NONE",
  "Return ONLY that — no prose, no quotes, no code fence.",
].join("\n");

export interface SkillImproveResult {
  improved: number;
  created: number;
}

export interface SkillImproveOpts {
  maxDistance?: number; // cosine cap for "this is the same skill"
  apply?: boolean;
  max?: number; // cap candidates processed per pass
}

/**
 * Refine existing skills / draft new ones from a batch of just-learned
 * memories. `apply=false` computes counts without writing (dry run).
 */
export async function improveSkills(
  client: MemnestClient,
  candidates: LearnedMemory[],
  llm: LlmComplete,
  opts: SkillImproveOpts = {},
): Promise<SkillImproveResult> {
  const maxDistance = opts.maxDistance ?? 0.25;
  const apply = opts.apply ?? true;
  const pool = candidates.filter(isSkillCandidate).slice(0, opts.max ?? 6);

  let improved = 0;
  let created = 0;
  for (const m of pool) {
    const ns = await client.neighbors({
      text: m.text,
      project: SKILL_PROJECT,
      k: 3,
      maxDistance,
    });
    if (ns.length > 0) {
      const target = ns[0]!;
      const refined = (
        await llm({
          system: SKILL_REFINE_SYSTEM_PROMPT,
          user: `CURRENT SKILL:\n${target.document}\n\nNEW LEARNING:\n${m.text}`,
        })
      ).trim();
      if (refined && refined.toUpperCase() !== "NONE") {
        if (apply) await client.update(target.id, { text: refined, chunkType: "consolidated" });
        improved++;
      }
    } else {
      const draft = (await llm({ system: SKILL_DRAFT_SYSTEM_PROMPT, user: m.text })).trim();
      if (draft && draft.toUpperCase() !== "NONE" && draft.startsWith("#")) {
        if (apply)
          await client.add({
            text: draft,
            project: SKILL_PROJECT,
            category: "convention",
            importance: "knowledge",
            chunkType: "manual",
          });
        created++;
      }
    }
  }
  return { improved, created };
}
