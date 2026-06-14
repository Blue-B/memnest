// User model deepening (Honcho-style dialectic).
//
// Hermes "builds a deepening model of who you are across sessions". memnest-learn
// captures preferences/corrections as flat memories, but never folds them into a
// single sharpening picture of the user. This module keeps a `_user_model`
// bucket of refined facets: each new preference/correction either refines the
// nearest existing facet (sharper, current — conflicts resolve toward the new
// observation) or is added as a new facet. `userModelContext` then surfaces the
// top facets into the injection block so the agent always knows who it serves.

import type { MemnestClient } from "./memnest-client.js";
import { trigramSimilarity } from "./consolidate.js";
import type { LearnedMemory, LlmComplete, MemoryCategory } from "./types.js";

export const USER_MODEL_PROJECT = "_user_model";

const FACET_CATS = new Set<MemoryCategory>(["preference", "correction"]);

export function isUserFacet(m: LearnedMemory): boolean {
  return FACET_CATS.has(m.category);
}

export const USER_MODEL_REFINE_SYSTEM_PROMPT = [
  "You maintain a concise, evolving model of one specific user — their",
  "preferences, conventions, and working style.",
  "Given an EXISTING facet and a NEW observation about the same aspect, return",
  "ONLY one improved facet sentence (no prose, no quotes, no code fence).",
  "Make it sharper and current; if the two conflict, prefer the NEW observation.",
].join("\n");

export interface UserModelResult {
  refined: number;
  added: number;
}

export interface UserModelOpts {
  maxDistance?: number; // cosine cap for "same aspect of the user"
  apply?: boolean;
  max?: number;
}

/**
 * Fold a batch of preference/correction memories into the evolving user model:
 * refine the nearest existing facet, or add a new one. Non-destructive
 * (refinement is an in-place /update). `apply=false` is a dry run.
 */
export async function updateUserModel(
  client: MemnestClient,
  facts: LearnedMemory[],
  llm: LlmComplete,
  opts: UserModelOpts = {},
): Promise<UserModelResult> {
  const maxDistance = opts.maxDistance ?? 0.22;
  const apply = opts.apply ?? true;
  const pool = facts.filter(isUserFacet).slice(0, opts.max ?? 6);

  let refined = 0;
  let added = 0;
  // Facets added in THIS pass aren't indexed yet, so the engine's /neighbors
  // can't dedup against them. Guard the intra-batch case locally so two
  // restated preferences in one capture don't both get inserted.
  const addedThisBatch: string[] = [];
  for (const f of pool) {
    if (addedThisBatch.some((t) => trigramSimilarity(t, f.text) >= 0.5)) continue;
    const ns = await client.neighbors({
      text: f.text,
      project: USER_MODEL_PROJECT,
      k: 3,
      maxDistance,
    });
    if (ns.length > 0) {
      const target = ns[0]!;
      const merged = (
        await llm({
          system: USER_MODEL_REFINE_SYSTEM_PROMPT,
          user: `EXISTING:\n${target.document}\n\nNEW:\n${f.text}`,
        })
      ).trim();
      if (merged) {
        if (apply) await client.update(target.id, { text: merged, importance: "preference" });
        refined++;
      }
    } else {
      if (apply)
        await client.add({
          text: f.text,
          project: USER_MODEL_PROJECT,
          category: f.category,
          importance: "preference",
          chunkType: "manual",
        });
      addedThisBatch.push(f.text);
      added++;
    }
  }
  return { refined, added };
}

/**
 * Build a compact "who you are" block from the top user-model facets, for the
 * byte-stable injection snapshot. Empty string when the model is empty.
 */
export async function userModelContext(
  client: MemnestClient,
  query: string,
  opts: { max?: number } = {},
): Promise<string> {
  const max = opts.max ?? 5;
  const hits = await client.search(query || "user preferences and working style", {
    project: USER_MODEL_PROJECT,
    nResults: max,
  });
  if (hits.length === 0) return "";
  const lines = hits.map((h) => `- ${h.document.replace(/\s+/g, " ").trim()}`);
  return ["user_profile:", ...lines].join("\n");
}
