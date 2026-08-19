/// <reference types="bun-types" />
import { expect, test } from "bun:test";

import { MemorySnapshot, type Clock } from "../src/kv-snapshot.js";
import {
  applyScratch,
  buildHandoff,
  formatScratchpad,
  parseScratchpad,
} from "../src/working-memory.js";
import { extractJsonArray, parseExtraction } from "../src/extract.js";
import {
  clusterBySimilarity,
  consolidate,
  consolidateByEmbedding,
  planClusterMerge,
  trigramSimilarity,
} from "../src/consolidate.js";
import { captureMemories } from "../src/capture.js";
import { MemnestClient, type FetchLike } from "../src/memnest-client.js";
import { rankByDurability } from "../src/types.js";
import type { SearchItem } from "../src/types.js";

// ── helpers ────────────────────────────────────────────────────────────────
function fakeClock(date: string, iso = `${date}T00:00:00.000Z`): Clock {
  return { isoNow: () => iso, today: () => date };
}
function item(id: string, document: string, score = 1): SearchItem {
  return { id, project: "p", document, score, timestamp: "", chunk_type: "Manual", importance: "Knowledge", category: "general" };
}

// ── KV-cache snapshot ───────────────────────────────────────────────────────
test("snapshot is byte-stable across turns until a checkpoint", async () => {
  let calls = 0;
  const builder = async () => `ctx#${++calls}`;
  const snap = new MemorySnapshot(fakeClock("2026-06-02"));
  const a = await snap.get(builder);
  const b = await snap.get(builder);
  expect(a.text).toBe("ctx#1");
  expect(b.text).toBe("ctx#1"); // no rebuild
  expect(calls).toBe(1);
});

// A memory write must NOT rebuild the snapshot: that would change the header
// bytes and drop the whole prompt prefix cache. Only a checkpoint may rebuild,
// and a day rollover is the one that fires on its own.
test("snapshot holds its bytes across turns and rebuilds on day rollover", async () => {
  let calls = 0;
  const builder = async () => `ctx#${++calls}`;
  const clock = { day: "2026-06-02", iso: "2026-06-02T00:00:00Z" };
  const snap = new MemorySnapshot({ isoNow: () => clock.iso, today: () => clock.day });
  await snap.get(builder); // build #1
  expect((await snap.get(builder)).text).toBe("ctx#1"); // no rebuild
  clock.day = "2026-06-03"; // day rollover
  const afterRollover = await snap.get(builder); // build #2
  expect(afterRollover.text).toBe("ctx#2");
  expect(afterRollover.reason).toBe("day_rollover");
  expect(calls).toBe(2);
});

// ── durability ranking (prompt-independent selection) ───────────────────────
// The injection block has no query to be relevant to, so it ranks by how
// durable a memory is: importance, then net recall feedback, then recency,
// then id. The id key makes the comparator a total order, which is what keeps
// the rendered block byte-identical across rebuilds over the same rows.
test("rankByDurability orders by importance, then feedback, then recency", () => {
  const row = (over: Partial<SearchItem>): SearchItem => ({
    id: "x", project: "playbook", document: "d", score: 0,
    timestamp: "2026-01-01T00:00:00Z", chunk_type: "Manual",
    importance: "Knowledge", category: "general", ...over,
  });
  // Engine casing is PascalCase on the way out; ranking must not care.
  const ranked = rankByDurability([
    row({ id: "log", importance: "Log" }),
    row({ id: "old-pref", importance: "Preference", timestamp: "2026-01-01T00:00:00Z" }),
    row({ id: "knowledge", importance: "Knowledge" }),
    row({ id: "new-pref", importance: "Preference", timestamp: "2026-08-01T00:00:00Z" }),
    row({ id: "decision", importance: "Decision" }),
    row({ id: "helpful-pref", importance: "Preference", helpful_count: 5 }),
  ]);
  expect(ranked.map((r) => r.id)).toEqual([
    "helpful-pref", // preference, and feedback outranks recency
    "new-pref",
    "old-pref",
    "decision",
    "knowledge",
    "log",
  ]);
});

test("rankByDurability is a total order and leaves its input alone", () => {
  const tied: SearchItem[] = ["c", "a", "b"].map((id) => ({
    id, project: "p", document: "d", score: 0, timestamp: "2026-01-01T00:00:00Z",
    chunk_type: "Manual", importance: "Preference", category: "preference",
  }));
  // Tied on every durability signal: the id tiebreak must still yield one fixed
  // order, or an unchanged bucket could render different bytes on each rebuild.
  expect(rankByDurability(tied).map((r) => r.id)).toEqual(["a", "b", "c"]);
  expect(rankByDurability(tied.slice().reverse()).map((r) => r.id)).toEqual(["a", "b", "c"]);
  expect(tied.map((r) => r.id)).toEqual(["c", "a", "b"]); // input untouched
  // A memory marked harmful sinks below an unrated one of equal importance.
  const rated = rankByDurability([
    { ...tied[0]!, id: "harmful", harmful_count: 3 },
    { ...tied[0]!, id: "neutral" },
  ]);
  expect(rated.map((r) => r.id)).toEqual(["neutral", "harmful"]);
});

// ── working memory ───────────────────────────────────────────────────────────
test("scratchpad parse/format round-trips and actions mutate by substring", () => {
  let items = parseScratchpad("# Scratchpad\n- [ ] fix auth\n- [x] ship docs\n");
  expect(items.length).toBe(2);
  expect(items[1]!.done).toBe(true);
  items = applyScratch(items, "add", "review PR 42");
  items = applyScratch(items, "done", "auth");
  expect(items.find((i) => i.text === "fix auth")!.done).toBe(true);
  expect(items.some((i) => i.text === "review PR 42")).toBe(true);
  // re-parsing the formatted output is stable
  expect(parseScratchpad(formatScratchpad(items)).length).toBe(3);
});

test("handoff captures open items + log tail, null when empty", () => {
  const h = buildHandoff("- [ ] fix auth\n- [x] done thing", "line1\nline2\nline3", "2026-06-02T10:00:00Z", "abcd", 2);
  expect(h).toContain("Session Handoff");
  expect(h).toContain("fix auth");
  expect(h).not.toContain("done thing"); // completed item excluded
  expect(h).toContain("line2\nline3"); // last 2 lines
  expect(buildHandoff("", "", "t", "s")).toBeNull();
});

// ── extraction parsing ───────────────────────────────────────────────────────
test("extractJsonArray tolerates fences and chatty prose", () => {
  const raw = 'Sure! Here:\n```json\n[{"category":"failure","text":"x"}]\n```\nhope that helps';
  expect(extractJsonArray(raw)).toEqual([{ category: "failure", text: "x" }]);
  expect(extractJsonArray("no array here")).toEqual([]);
});

test("parseExtraction validates category, keeps importance, drops junk", () => {
  const raw = JSON.stringify([
    { category: "correction", text: "Use pnpm not npm", importance: "decision" },
    { category: "bogus", text: "fallback to general" },
    { category: "failure", text: "" }, // empty -> dropped
    { text: "no category -> general" },
    "garbage",
  ]);
  const got = parseExtraction(raw);
  expect(got.length).toBe(3);
  expect(got[0]).toEqual({ category: "correction", text: "Use pnpm not npm", importance: "decision" });
  expect(got[1]!.category).toBe("general");
  expect(got[2]!.category).toBe("general");
});

// ── consolidation ────────────────────────────────────────────────────────────
test("trigram similarity: identical high, disjoint low", () => {
  expect(trigramSimilarity("the cat sat", "the cat sat")).toBe(1);
  expect(trigramSimilarity("the cat sat on the mat", "the cat sat upon the mat")).toBeGreaterThan(0.5);
  expect(trigramSimilarity("alpha beta", "zulu yankee")).toBeLessThan(0.2);
});

test("clusterBySimilarity groups near-dupes, isolates distinct", () => {
  const items = [
    item("a", "Deploy failed because a worker held the migration lock"),
    item("b", "The deploy failed since a worker was holding the migration lock"),
    item("c", "User prefers dark theme in the editor"),
  ];
  const clusters = clusterBySimilarity(items, 0.4).map((cl) => cl.map((i) => i.id).sort());
  expect(clusters).toContainEqual(["a", "b"]);
  expect(clusters).toContainEqual(["c"]);
});

test("consolidate dry-run plans without writing; apply updates + supersedes", async () => {
  const items = [
    item("a", "Deploy failed because a worker held the migration lock", 0.9),
    item("b", "Deploy failed since a worker was holding the migration lock", 0.8),
  ];
  const llm = async () => "Deploy failed: a stuck worker held the migration lock.";
  const calls: string[] = [];
  const client = {
    update: async (id: string) => calls.push(`update:${id}`),
    supersede: async (id: string) => calls.push(`supersede:${id}`),
  } as unknown as MemnestClient;

  const dry = await consolidate(client, items, llm, { threshold: 0.4, apply: false });
  expect(dry.merged).toBe(1);
  expect(dry.superseded).toBe(1);
  expect(calls.length).toBe(0); // nothing written on dry run

  const applied = await consolidate(client, items, llm, { threshold: 0.4, apply: true });
  expect(applied.merged).toBe(1);
  expect(calls).toEqual(["update:a", "supersede:b"]); // keep highest-ranked, retire the rest
});

test("planClusterMerge returns null for singletons", async () => {
  expect(await planClusterMerge([item("a", "x")], async () => "y")).toBeNull();
});

test("consolidateByEmbedding clusters via engine cosine neighbors", async () => {
  const items = [
    item("a", "Deploy failed: a stuck worker held the migration lock", 0.9),
    item("b", "Our deploy broke because a hung worker kept the migration lock", 0.8),
    item("c", "User prefers a dark theme in the editor", 0.7),
  ];
  // engine says a<->b are cosine-near; c is isolated (trigram would miss a/b)
  const adj: Record<string, string[]> = { a: ["b"], b: ["a"], c: [] };
  const calls: string[] = [];
  const client = {
    neighbors: async ({ id }: { id: string }) =>
      (adj[id] ?? []).map((nid) => ({ id: nid, project: "p", document: "", distance: 0.1, category: "", importance: "", chunk_type: "" })),
    update: async (id: string) => calls.push(`update:${id}`),
    supersede: async (id: string) => calls.push(`supersede:${id}`),
  } as unknown as MemnestClient;
  const llm = async () => "Deploy broke because a worker held the migration lock.";
  const res = await consolidateByEmbedding(client, items, llm, { maxDistance: 0.25, apply: true });
  expect(res.clusters).toBe(1);
  expect(res.merged).toBe(1);
  expect(res.superseded).toBe(1);
  expect(calls).toEqual(["update:a", "supersede:b"]);
});

// ── capture ──────────────────────────────────────────────────────────────────
test("captureMemories writes extracted memories with category + default importance", async () => {
  const llm = async () =>
    JSON.stringify([
      { category: "preference", text: "Prefers dark theme" }, // -> importance preference
      { category: "failure", text: "localStorage tokens are XSS-prone" }, // -> knowledge
    ]);
  const added: any[] = [];
  const client = {
    add: async (input: any) => {
      added.push(input);
      return { status: "queued", id: `id${added.length}`, project: input.project };
    },
  } as unknown as MemnestClient;

  const res = await captureMemories([{ role: "user", text: "hi" }], llm, client, { project: "proj" });
  expect(res.extracted).toBe(2);
  expect(res.written).toEqual(["id1", "id2"]);
  expect(added[0]).toMatchObject({ category: "preference", importance: "preference" });
  expect(added[1]).toMatchObject({ category: "failure", importance: "knowledge", project: "proj" });
});

// buildInjection's learned_rules slot searches ONLY `playbook`. The periodic
// capture pass is now the sole path that feeds it, so a durable lesson
// (correction -> decision, preference -> preference) must be routed there;
// weaker memories stay project-local.
test("captureMemories routes durable lessons to playbook, keeps the rest project-local", async () => {
  const llm = async () =>
    JSON.stringify([
      { category: "correction", text: "Verify the adapter with Get-NetAdapter instead of assuming WiFi" },
      { category: "preference", text: "Prefers pnpm over npm" },
      { category: "failure", text: "localStorage tokens are XSS-prone" },
      { category: "general", text: "the repo has a learn/ workspace" },
    ]);
  const added: any[] = [];
  const client = {
    add: async (input: any) => {
      added.push(input);
      return { status: "queued", id: `id${added.length}`, project: input.project };
    },
  } as unknown as MemnestClient;

  await captureMemories([{ role: "user", text: "hi" }], llm, client, { project: "proj" });
  expect(added.map((a) => [a.category, a.importance, a.project])).toEqual([
    ["correction", "decision", "playbook"],
    ["preference", "preference", "playbook"],
    ["failure", "knowledge", "proj"],
    ["general", "log", "proj"],
  ]);
});

// ── memnest client (fake fetch) ──────────────────────────────────────────────
test("client.add posts correct metadata; search parses; supersede re-parents", async () => {
  const seen: { url: string; body: any }[] = [];
  const fetchFn: FetchLike = async (url, init) => {
    const body = init?.body ? JSON.parse(init.body) : {};
    seen.push({ url, body });
    if (url.endsWith("/search")) {
      return { ok: true, status: 200, json: async () => ({ results: [item("x", "doc")] }), text: async () => "" };
    }
    return { ok: true, status: 200, json: async () => ({ status: "queued", id: "id1", project: body.project }), text: async () => "" };
  };
  const client = new MemnestClient("http://localhost:3111", fetchFn);

  await client.add({ text: "hello", project: "proj", category: "insight" });
  expect(seen[0]!.url).toBe("http://localhost:3111/add");
  expect(seen[0]!.body.metadata).toMatchObject({ category: "insight", chunk_type: "manual" });

  const results = await client.search("q", { project: "proj" });
  expect(results[0]!.id).toBe("x");

  await client.supersede("id9");
  const last = seen[seen.length - 1]!;
  expect(last.url.endsWith("/update")).toBe(true);
  expect(last.body).toMatchObject({ id: "id9", project: "_superseded", chunk_type: "consolidated" });
});
