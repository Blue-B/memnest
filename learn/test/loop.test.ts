/// <reference types="bun-types" />
import { expect, test } from "bun:test";

import {
  bumpImportance,
  detectOutcomeSignal,
  recurrenceCount,
  reinforce,
  withRecurrenceMarker,
} from "../src/reinforce.js";
import { improveSkills, isSkillCandidate } from "../src/skills.js";
import { LlmBudget } from "../src/budget.js";
import { extractMessageText } from "../src/capture.js";
import { isUserFacet, updateUserModel, userModelContext } from "../src/user-model.js";
import type { MemnestClient, NeighborItem } from "../src/memnest-client.js";
import type { LearnedMemory } from "../src/types.js";

// ── helpers ──────────────────────────────────────────────────────────────────
function neighbor(over: Partial<NeighborItem> = {}): NeighborItem {
  return {
    id: "n1",
    project: "default",
    document: "doc",
    distance: 0.1,
    category: "failure",
    importance: "knowledge",
    chunk_type: "manual",
    ...over,
  };
}

// ── importance ladder ─────────────────────────────────────────────────────────
test("bumpImportance steps up one rung, honours cap, tolerates capitalised input", () => {
  expect(bumpImportance("log")).toBe("knowledge");
  expect(bumpImportance("knowledge")).toBe("decision");
  expect(bumpImportance("Knowledge")).toBe("decision"); // engine returns Capitalised
  expect(bumpImportance("decision", "decision")).toBe("decision"); // capped
  expect(bumpImportance("preference")).toBe("preference"); // already top
  expect(bumpImportance("garbage")).toBe("knowledge"); // unknown -> safe default
});

// ── recurrence marker ─────────────────────────────────────────────────────────
test("recurrence marker round-trips and is idempotent on re-stamp", () => {
  expect(recurrenceCount("plain text")).toBe(0);
  const once = withRecurrenceMarker("Deploy lock bug", 1);
  expect(once).toBe("Deploy lock bug [recurred ×1]");
  expect(recurrenceCount(once)).toBe(1);
  const twice = withRecurrenceMarker(once, 2);
  expect(twice).toBe("Deploy lock bug [recurred ×2]"); // not doubled
  expect(recurrenceCount(twice)).toBe(2);
});

// ── outcome detection ─────────────────────────────────────────────────────────
test("detectOutcomeSignal classifies recurrence / success (EN + KO), else null", () => {
  expect(detectOutcomeSignal("it's still broken")).toBe("recurrence");
  expect(detectOutcomeSignal("same error again")).toBe("recurrence");
  expect(detectOutcomeSignal("아직도 안 고쳐졌어")).toBe("recurrence");
  expect(detectOutcomeSignal("여전히 똑같네")).toBe("recurrence");
  expect(detectOutcomeSignal("the dev server is still dying")).toBe("recurrence");
  expect(detectOutcomeSignal("that port 5173 thing is back again")).toBe("recurrence");
  expect(detectOutcomeSignal("it keeps crashing")).toBe("recurrence");
  expect(detectOutcomeSignal("또 죽었어")).toBe("recurrence");
  expect(detectOutcomeSignal("let's go back to the old approach")).toBeNull(); // 'back' must not over-fire
  expect(detectOutcomeSignal("works now, thanks")).toBe("success");
  expect(detectOutcomeSignal("오 이제 잘 돼")).toBe("success");
  expect(detectOutcomeSignal("해결됐다")).toBe("success");
  expect(detectOutcomeSignal("add a test please")).toBeNull();
});

// ── reinforcement action (fake client) ────────────────────────────────────────
test("reinforce(recurrence) bumps the matching failure + stamps recurrence", async () => {
  const updates: any[] = [];
  const client = {
    neighbors: async () => [
      neighbor({ id: "f1", category: "Failure", importance: "knowledge", document: "Deploy lock bug" }),
      neighbor({ id: "p1", category: "preference", importance: "preference", document: "dark theme" }),
    ],
    update: async (id: string, fields: any) => updates.push({ id, fields }),
  } as unknown as MemnestClient;

  const r = await reinforce(client, "recurrence", "the deploy lock thing is still broken");
  expect(r).toMatchObject({ matched: true, id: "f1", action: "reinforced", recurred: 1, newImportance: "decision" });
  expect(updates).toHaveLength(1);
  expect(updates[0].fields.text).toBe("Deploy lock bug [recurred ×1]");
  expect(updates[0].fields.importance).toBe("decision");
});

test("reinforce(success) validates by one step; no-op when nothing close", async () => {
  const updates: any[] = [];
  const close = {
    neighbors: async () => [neighbor({ id: "i1", category: "insight", importance: "knowledge" })],
    update: async (id: string, fields: any) => updates.push({ id, fields }),
  } as unknown as MemnestClient;
  const r = await reinforce(close, "success", "great, it works now");
  expect(r).toMatchObject({ matched: true, id: "i1", action: "validated", newImportance: "decision" });
  expect(updates).toHaveLength(1);

  const none = { neighbors: async () => [], update: async () => {} } as unknown as MemnestClient;
  expect(await reinforce(none, "recurrence", "still broken")).toEqual({ matched: false });
  expect(await reinforce(none, null, "anything")).toEqual({ matched: false });
});

// ── background budget / throttle ──────────────────────────────────────────────
test("LlmBudget caps calls per window and refills as the window slides", () => {
  let t = 1000;
  const b = new LlmBudget(3, 100, () => t);
  expect(b.allow()).toBe(true);
  expect(b.allow()).toBe(true);
  expect(b.allow()).toBe(true);
  expect(b.allow()).toBe(false); // 3/3 used within the window
  t += 101; // window slides past the first three
  expect(b.allow()).toBe(true);
  expect(b.state()).toMatchObject({ used: 1, max: 3 });
});

test("LlmBudget with maxCalls<=0 never allows", () => {
  const b = new LlmBudget(0, 1000);
  expect(b.allow()).toBe(false);
});

// ── assistant-text extraction (agent_end capture) ─────────────────────────────
test("extractMessageText handles string, content blocks, skips non-text", () => {
  expect(extractMessageText("plain")).toBe("plain");
  expect(
    extractMessageText([
      { type: "text", text: "first" },
      { type: "tool_use", id: "x" },
      { type: "text", text: "second" },
    ]),
  ).toBe("first\nsecond");
  expect(extractMessageText([{ type: "image", source: {} }])).toBe("");
  expect(extractMessageText(null)).toBe("");
});

// ── skill self-improvement (fake client) ──────────────────────────────────────
test("isSkillCandidate gates on procedural categories", () => {
  expect(isSkillCandidate({ category: "convention", text: "x" })).toBe(true);
  expect(isSkillCandidate({ category: "insight", text: "x" })).toBe(true);
  expect(isSkillCandidate({ category: "preference", text: "x" })).toBe(false);
  expect(isSkillCandidate({ category: "general", text: "x" })).toBe(false);
});

test("improveSkills refines a near skill and drafts a new one otherwise", async () => {
  const updates: any[] = [];
  const adds: any[] = [];
  const client = {
    neighbors: async ({ text }: { text: string }) =>
      text.includes("lockfile")
        ? [neighbor({ id: "s1", project: "_skills", document: "# Install\n1. run npm i" })]
        : [],
    update: async (id: string, fields: any) => updates.push({ id, fields }),
    add: async (input: any) => {
      adds.push(input);
      return { status: "queued", id: `a${adds.length}`, project: input.project };
    },
  } as unknown as MemnestClient;

  const candidates: LearnedMemory[] = [
    { category: "convention", text: "commit the lockfile after every install" }, // -> refine s1
    { category: "tool_quirk", text: "the bundler needs --force on first run" }, // -> draft new
    { category: "preference", text: "likes dark mode" }, // -> ignored (not a candidate)
  ];
  const llm = async ({ user }: { user: string }) =>
    user.startsWith("CURRENT SKILL") ? "# Install\n1. run npm i\n2. commit the lockfile" : "# Bundler\n1. run with --force the first time";

  const res = await improveSkills(client, candidates, llm);
  expect(res).toEqual({ improved: 1, created: 1 });
  expect(updates[0].id).toBe("s1");
  expect(updates[0].fields.text).toContain("commit the lockfile");
  expect(adds[0].project).toBe("_skills");
  expect(adds[0].text.startsWith("#")).toBe(true);
});

test("improveSkills drops a non-procedural draft (LLM returns NONE)", async () => {
  const adds: any[] = [];
  const client = {
    neighbors: async () => [],
    add: async (i: any) => (adds.push(i), { id: "x", project: i.project, status: "ok" }),
    update: async () => {},
  } as unknown as MemnestClient;
  const res = await improveSkills(client, [{ category: "insight", text: "the sky is blue" }], async () => "NONE");
  expect(res).toEqual({ improved: 0, created: 0 });
  expect(adds).toHaveLength(0);
});

// ── user model (fake client) ──────────────────────────────────────────────────
test("isUserFacet accepts only preference (correction was too polluting)", () => {
  expect(isUserFacet({ category: "preference", text: "x" })).toBe(true);
  expect(isUserFacet({ category: "correction", text: "x" })).toBe(false); // technical fixes leaked in
  expect(isUserFacet({ category: "failure", text: "x" })).toBe(false);
});

test("updateUserModel refines an existing facet, adds a new one", async () => {
  const updates: any[] = [];
  const adds: any[] = [];
  const client = {
    neighbors: async ({ text }: { text: string }) =>
      text.includes("package manager")
        ? [neighbor({ id: "u1", project: "_user_model", document: "User prefers npm" })]
        : [],
    update: async (id: string, fields: any) => updates.push({ id, fields }),
    add: async (i: any) => (adds.push(i), { id: "ua", project: i.project, status: "ok" }),
  } as unknown as MemnestClient;

  const facts: LearnedMemory[] = [
    { category: "preference", text: "Use bun, not the npm package manager" }, // refine u1 (matches "package manager")
    { category: "preference", text: "Writes commit messages in Korean" }, // add
    { category: "failure", text: "ignored, not a facet" },
    { category: "correction", text: "ignored now — corrections no longer feed the user model" },
  ];
  const llm = async () => "User prefers Bun over npm as the package manager";
  const res = await updateUserModel(client, facts, llm);
  expect(res).toEqual({ refined: 1, added: 1 });
  expect(updates[0].id).toBe("u1");
  expect(updates[0].fields.importance).toBe("preference");
  expect(adds[0].project).toBe("_user_model");
});

test("updateUserModel dedups two restated facets within one batch", async () => {
  const adds: any[] = [];
  const client = {
    neighbors: async () => [], // engine sees nothing (not indexed yet)
    add: async (i: any) => (adds.push(i), { id: `u${adds.length}`, project: i.project, status: "ok" }),
    update: async () => {},
  } as unknown as MemnestClient;
  const facts: LearnedMemory[] = [
    { category: "preference", text: "User prefers Bun as the package manager" },
    { category: "preference", text: "User prefers Bun as the package manager for everything" }, // restated
  ];
  const res = await updateUserModel(client, facts, async () => "merged");
  expect(res.added).toBe(1); // second restatement skipped, not double-inserted
  expect(adds).toHaveLength(1);
});

test("userModelContext formats top facets, empty when none", async () => {
  const withFacets = {
    search: async () => [
      { id: "1", project: "_user_model", document: "Prefers Bun", score: 1, timestamp: "", chunk_type: "manual", importance: "preference", category: "preference" },
    ],
  } as unknown as MemnestClient;
  const block = await userModelContext(withFacets, "");
  expect(block).toBe("user_profile:\n- Prefers Bun");

  const empty = { search: async () => [] } as unknown as MemnestClient;
  expect(await userModelContext(empty, "")).toBe("");
});
