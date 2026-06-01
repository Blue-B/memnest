// Consolidation: find textually-redundant memories and merge each cluster into
// one canonical entry, retiring the rest non-destructively (mem0-style
// extract-and-merge, but as a client job — the engine core stays LLM-free).
//
// Clustering is pure (trigram Jaccard over the returned documents — the engine
// returns a composite score, not a cosine, so we can't reuse that for
// similarity). The merge itself uses the injected host LLM.

import type { MemnestClient } from "./memnest-client.js";
import type { LlmComplete, SearchItem } from "./types.js";

export interface MergePlan {
  keepId: string;
  mergedText: string;
  supersededIds: string[];
}

function trigrams(s: string): Set<string> {
  const norm = s.toLowerCase().replace(/\s+/g, " ").trim();
  if (norm.length < 3) return new Set([norm]);
  const set = new Set<string>();
  for (let i = 0; i <= norm.length - 3; i++) set.add(norm.slice(i, i + 3));
  return set;
}

export function trigramSimilarity(a: string, b: string): number {
  const ta = trigrams(a);
  const tb = trigrams(b);
  if (ta.size === 0 && tb.size === 0) return 1;
  let inter = 0;
  for (const t of ta) if (tb.has(t)) inter++;
  const union = ta.size + tb.size - inter;
  return union === 0 ? 0 : inter / union;
}

/** Greedy single-link clustering by document similarity (>= threshold). */
export function clusterBySimilarity(items: SearchItem[], threshold: number): SearchItem[][] {
  const clusters: SearchItem[][] = [];
  const used = new Set<number>();
  for (let i = 0; i < items.length; i++) {
    if (used.has(i)) continue;
    const cluster = [items[i]!];
    used.add(i);
    for (let j = i + 1; j < items.length; j++) {
      if (used.has(j)) continue;
      if (cluster.some((c) => trigramSimilarity(c.document, items[j]!.document) >= threshold)) {
        cluster.push(items[j]!);
        used.add(j);
      }
    }
    clusters.push(cluster);
  }
  return clusters;
}

const MERGE_SYSTEM_PROMPT = [
  "You merge several near-duplicate memory entries into ONE.",
  "Return ONLY the merged memory text (no prose, no quotes, no code fence).",
  "Preserve every distinct fact across the inputs; drop pure repetition.",
  "Keep it concise and self-contained.",
].join("\n");

/** Ask the host LLM to merge a cluster (size >= 2) into one canonical entry. */
export async function planClusterMerge(
  cluster: SearchItem[],
  llm: LlmComplete,
): Promise<MergePlan | null> {
  if (cluster.length < 2) return null;
  // Keep the highest-ranked item as the survivor (engine already sorted desc).
  const keep = cluster[0]!;
  const merged = (
    await llm({
      system: MERGE_SYSTEM_PROMPT,
      user: cluster.map((c, i) => `(${i + 1}) ${c.document}`).join("\n"),
    })
  ).trim();
  if (!merged) return null;
  return {
    keepId: keep.id,
    mergedText: merged,
    supersededIds: cluster.slice(1).map((c) => c.id),
  };
}

export interface ConsolidateResult {
  clusters: number;
  merged: number;
  superseded: number;
}

/**
 * Cluster `items`, merge each redundant cluster via the LLM, then update the
 * survivor and park the rest in `_superseded` (reversible). `apply=false`
 * yields a dry run (plans computed, nothing written).
 */
export async function consolidate(
  client: MemnestClient,
  items: SearchItem[],
  llm: LlmComplete,
  opts: { threshold?: number; apply?: boolean } = {},
): Promise<ConsolidateResult> {
  const threshold = opts.threshold ?? 0.6;
  const apply = opts.apply ?? true;
  const clusters = clusterBySimilarity(items, threshold).filter((c) => c.length >= 2);
  let merged = 0;
  let superseded = 0;
  for (const cluster of clusters) {
    const plan = await planClusterMerge(cluster, llm);
    if (!plan) continue;
    if (apply) {
      await client.update(plan.keepId, { text: plan.mergedText, chunkType: "consolidated" });
      for (const id of plan.supersededIds) await client.supersede(id);
    }
    merged++;
    superseded += plan.supersededIds.length;
  }
  return { clusters: clusters.length, merged, superseded };
}
