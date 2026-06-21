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
import { captureCorrection, captureMemories, looksLikeCorrection } from "../src/capture.js";
import { MemnestClient, type FetchLike } from "../src/memnest-client.js";
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

test("snapshot rebuilds on markDirty and on day rollover", async () => {
  let calls = 0;
  const builder = async () => `ctx#${++calls}`;
  const clock = { day: "2026-06-02", iso: "2026-06-02T00:00:00Z" };
  const snap = new MemorySnapshot({ isoNow: () => clock.iso, today: () => clock.day });
  await snap.get(builder); // build #1
  snap.markDirty();
  const afterDirty = await snap.get(builder); // build #2
  expect(afterDirty.text).toBe("ctx#2");
  clock.day = "2026-06-03"; // day rollover
  const afterRollover = await snap.get(builder); // build #3
  expect(afterRollover.text).toBe("ctx#3");
  expect(calls).toBe(3);
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
test("looksLikeCorrection detects EN + KO correction candidates, ignores neutral", () => {
  expect(looksLikeCorrection("No, use pnpm not npm")).toBe(true);
  expect(looksLikeCorrection("아니 그거 틀렸어")).toBe(true);
  expect(looksLikeCorrection("추측하지말고 직접 확인했어야지")).toBe(true);
  expect(looksLikeCorrection("please add a test for this")).toBe(false);
});

test("looksLikeCorrection avoids broad Korean false positives", () => {
  expect(looksLikeCorrection("자동로그말고 교정기록이 너무 자주 작동하는데? ")).toBe(false);
  expect(looksLikeCorrection("그러면 그냥 지금 기존 pi랑 똑같은 거 아니야?")).toBe(false);
  expect(looksLikeCorrection("이미 스마트발송은 다 되어있잖아 근데 뭘 해야 한단 거야?")).toBe(false);
  expect(looksLikeCorrection("응 진행해 근데 룰베이스말고 더 효율적인 설계는 없어?")).toBe(false);
});

test("looksLikeCorrection detects KO skepticism / failure-prediction", () => {
  // A doubt that the agent will repeat a past mistake = a correction signal.
  expect(looksLikeCorrection("이번에도 그럴 것 같아")).toBe(true);
  expect(looksLikeCorrection("이번에도그럴꺼같아")).toBe(true);
  expect(looksLikeCorrection("또 실패할 것 같아")).toBe(true);
  expect(looksLikeCorrection("이것도 안될것같아")).toBe(true);
  expect(looksLikeCorrection("해도 의미없을것같은데")).toBe(true);
});

test("looksLikeCorrection does NOT match positive '~ㄹ 것 같아' (no false positives)", () => {
  // The narrow negative-pointer design must leave optimistic agreement alone,
  // otherwise every approving turn would spuriously fire correction capture.
  expect(looksLikeCorrection("이거 맞는 것 같아 진행해줘")).toBe(false);
  expect(looksLikeCorrection("이렇게 하면 될 것 같아")).toBe(false);
  expect(looksLikeCorrection("지금 잘 되고 있는 것 같아")).toBe(false);
  expect(looksLikeCorrection("다음 단계로 가도 될 것 같아")).toBe(false);
});

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
  expect(added[0]).toMatchObject({ category: "preference", importance: "preference", project: "proj" });
  expect(added[1]).toMatchObject({ category: "failure", importance: "knowledge" });
});

test("captureCorrection stores the raw complaint as a decision when no LLM is given", async () => {
  let captured: any = null;
  const client = {
    add: async (i: any) => {
      captured = i;
      return { id: "c1" };
    },
  } as unknown as MemnestClient;
  const res = await captureCorrection("Use pnpm, not npm", client, "proj");
  expect(res).toMatchObject({ id: "c1", distilled: false, lesson: "Use pnpm, not npm" });
  // raw (non-distilled) complaint stays project-local so it can't pollute playbook
  expect(captured).toMatchObject({ category: "correction", importance: "decision", text: "Use pnpm, not npm", project: "proj" });
  expect(await captureCorrection("   ", client)).toBeNull();
});

test("captureCorrection distils a lesson from context into a preference when an LLM is given", async () => {
  let captured: any = null;
  const client = {
    add: async (i: any) => {
      captured = i;
      return { id: "c2" };
    },
  } as unknown as MemnestClient;
  const llm = async () => "네트워크를 추정하지 말고 Get-NetAdapter로 직접 확인할 것";
  const res = await captureCorrection("너 왜 또 추정해?", client, "proj", {
    llm,
    context: [
      { role: "user", text: "인터넷 속도 조회해줘" },
      { role: "assistant", text: "WiFi 쓰시는 것 같아요" },
    ],
  });
  expect(res).toMatchObject({ id: "c2", distilled: true });
  expect(res!.lesson).toContain("Get-NetAdapter");
  // a distilled rule is a durable cross-project lesson -> curated playbook bucket,
  // which is the ONLY bucket buildInjection's learned_rules slot reads
  expect(captured).toMatchObject({ category: "correction", importance: "preference", project: "playbook" });
  expect(captured.text).toContain("Get-NetAdapter");
});

test("captureCorrection drops ambiguous candidates when the LLM returns NONE", async () => {
  let captured: any = null;
  const client = {
    add: async (i: any) => {
      captured = i;
      return { id: "c3" };
    },
  } as unknown as MemnestClient;
  const llm = async () => "NONE";
  const res = await captureCorrection("아니 지금은 수정전 상태 세션인 거 아니야?", client, "proj", {
    llm,
    context: [{ role: "user", text: "x" }],
  });
  expect(res).toBeNull();
  expect(captured).toBeNull();
});

test("captureCorrection trusts semantic NONE even for high-confidence candidates", async () => {
  let captured: any = null;
  const client = {
    add: async (i: any) => {
      captured = i;
      return { id: "c4" };
    },
  } as unknown as MemnestClient;
  const llm = async () => "NONE";
  const res = await captureCorrection("추측하지말고 직접 확인했어야지", client, "proj", {
    llm,
    context: [{ role: "assistant", text: "아마 WiFi 문제 같습니다" }],
  });
  expect(res).toBeNull();
  expect(captured).toBeNull();
});

// If the classifier is unavailable, only high-confidence signals may fall back
// to raw storage. This preserves urgent corrections without letting particles
// like "잖아" / "말고" flood the playbook.
test("captureCorrection keeps high-confidence raw fallback when the LLM fails", async () => {
  let captured: any = null;
  const client = {
    add: async (i: any) => {
      captured = i;
      return { id: "c5" };
    },
  } as unknown as MemnestClient;
  const llm = async () => {
    throw new Error("LLM unavailable");
  };
  const res = await captureCorrection("추측하지말고 직접 확인했어야지", client, "proj", {
    llm,
    context: [{ role: "assistant", text: "아마 WiFi 문제 같습니다" }],
  });
  expect(res).toMatchObject({ id: "c5", distilled: false });
  expect(captured).toMatchObject({ category: "correction", importance: "decision", project: "proj" });
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
