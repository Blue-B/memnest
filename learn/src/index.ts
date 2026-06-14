// memnest-learn — the learning / working-memory / injection layer.
//
// pi extension entry point. It wires the unit-tested pure core (capture,
// consolidate, kv-snapshot, working-memory, memnest-client) to the pi runtime.
// The memnest engine stays LLM-free; every LLM call here borrows the host
// agent's own model via @earendil-works/pi-ai `complete`, so there is no extra
// API key, cost, or service to run.
//
// Hooks (surface verified against jayzeng/pi-memory + chandra447/pi-hermes):
//   session_start          -> build the byte-stable injection snapshot
//   before_agent_start     -> inject snapshot (memnest /context + working memory)
//   input                  -> turn counter + correction fast-path
//   session_before_compact -> write handoff, refresh snapshot
//   session_shutdown       -> final capture flush
// Tools: scratchpad (checklist), skill (procedural how-to, stored in memnest).

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { complete } from "@earendil-works/pi-ai";
import { Type } from "@sinclair/typebox";

import { MemnestClient } from "./memnest-client.js";
import { MemorySnapshot } from "./kv-snapshot.js";
import { captureCorrection, captureMemories, extractMessageText, looksLikeCorrection } from "./capture.js";
import { consolidateByEmbedding } from "./consolidate.js";
import { LlmBudget } from "./budget.js";
import { detectOutcomeSignal, reinforce } from "./reinforce.js";
import { improveSkills } from "./skills.js";
import { updateUserModel, userModelContext } from "./user-model.js";
import type { LearnedMemory } from "./types.js";
import {
  appendDaily,
  applyScratch,
  buildHandoff,
  formatScratchpad,
  parseScratchpad,
} from "./working-memory.js";
import type { LlmComplete, TranscriptTurn } from "./types.js";

// ── config / paths ───────────────────────────────────────────────────────────
const MEMNEST_URL = process.env.MEMNEST_URL ?? "http://127.0.0.1:3111";
const LEARN_DIR =
  process.env.MEMNEST_LEARN_DIR ?? path.join(os.homedir(), ".pi", "agent", "memnest-learn");
const SCRATCHPAD_FILE = path.join(LEARN_DIR, "SCRATCHPAD.md");
const DAILY_DIR = path.join(LEARN_DIR, "daily");
const CAPTURE_EVERY_TURNS = Number(process.env.MEMNEST_CAPTURE_TURNS ?? 10);
const SKILL_PROJECT = "_skills";
// Background LLM budget: cap automatic capture/skill/user-model calls so they
// can't compete with the user's real work. Manual tools are NOT gated.
const LLM_MAX_CALLS = Number(process.env.MEMNEST_LLM_MAX_CALLS ?? 24);
const LLM_WINDOW_MS = Number(process.env.MEMNEST_LLM_WINDOW_MS ?? 5 * 60 * 1000);

const client = new MemnestClient(MEMNEST_URL, fetch as any);
const snapshot = new MemorySnapshot();
const budget = new LlmBudget(LLM_MAX_CALLS, LLM_WINDOW_MS);

let turnCounter = 0;
let recentTurns: TranscriptTurn[] = [];
let bgInFlight = false; // one background learn pass at a time
const MAX_RECENT_TURNS = 120;

// ── small fs helpers ───────────────────────────────────────────────────────
function ensureDirs() {
  fs.mkdirSync(DAILY_DIR, { recursive: true });
}
function readSafe(p: string): string {
  try {
    return fs.readFileSync(p, "utf-8");
  } catch {
    return "";
  }
}
const today = () => new Date().toISOString().slice(0, 10);
const isoNow = () => new Date().toISOString();
const dailyPath = (d: string) => path.join(DAILY_DIR, `${d}.md`);
const shortSid = (ctx: ExtensionContext) => ctx.sessionManager.getSessionId().slice(0, 8);

// ── host-LLM bridge ──────────────────────────────────────────────────────────
function makeLlm(ctx: ExtensionContext): LlmComplete | null {
  if (!ctx.model) return null;
  return async ({ system, user }) => {
    const res = await complete(
      ctx.model!,
      { systemPrompt: system, messages: [{ role: "user", content: [{ type: "text", text: user }], timestamp: Date.now() }] },
      { reasoningEffort: "low" },
    );
    return res.content
      .filter((c): c is { type: "text"; text: string } => c.type === "text" && typeof c.text === "string")
      .map((c) => c.text)
      .join("\n")
      .trim();
  };
}

/**
 * Budget-gated LLM for BACKGROUND work. When the window is exhausted it returns
 * "" instead of calling the model; every downstream step (extraction, skill
 * draft/refine, user-model refine, consolidation) already treats an empty reply
 * as "nothing", so throttling degrades gracefully to a no-op.
 */
function backgroundLlm(ctx: ExtensionContext): LlmComplete | null {
  const llm = makeLlm(ctx);
  if (!llm) return null;
  return async (input) => (budget.allow() ? llm(input) : "");
}

// ── learning fan-out: route freshly-captured memories into the loops ─────────
// skill self-improvement (#2) + user-model deepening (#4). Best-effort; the
// outcome-reinforcement loop (#1) runs separately off the input signal.
async function learnFromMemories(memories: LearnedMemory[], llm: LlmComplete): Promise<void> {
  if (memories.length === 0) return;
  await Promise.allSettled([
    improveSkills(client, memories, llm, { max: 4 }),
    updateUserModel(client, memories, llm, { max: 4 }),
  ]);
}

// ── injection block (byte-stable) ────────────────────────────────────────────
async function buildInjection(prompt: string): Promise<string> {
  const parts: string[] = [];
  // 1) who the user is (deepening user model) — kept first so it's always seen
  try {
    const profile = await userModelContext(client, prompt || "user preferences working style");
    if (profile.trim()) parts.push(profile);
  } catch {
    /* user model unavailable — skip */
  }
  // 2) open scratchpad items (working memory)
  const open = parseScratchpad(readSafe(SCRATCHPAD_FILE)).filter((i) => !i.done);
  if (open.length > 0) {
    parts.push("open_tasks:");
    for (const i of open) parts.push(`- [ ] ${i.text}`);
  }
  // 3) memnest budget-bounded context pack (notes + facts + retrieved memories)
  try {
    const { prompt: pack } = await client.context(prompt || "recent work", { maxChars: 4000 });
    if (pack.trim()) parts.push(pack);
  } catch {
    /* memnest unreachable — degrade to working memory only */
  }
  return parts.join("\n");
}

export default function (pi: ExtensionAPI) {
  // session_start: prime the snapshot
  pi.on("session_start", async () => {
    ensureDirs();
    await snapshot.refresh("session_start", () => buildInjection(""));
  });

  // before_agent_start: inject the byte-stable memory block
  pi.on("before_agent_start", async (event: { prompt?: string; systemPrompt: string }) => {
    const { text, reason, takenAt } = await snapshot.get(() => buildInjection(event.prompt ?? ""));
    if (!text.trim()) return;
    const header = [
      "\n\n## Memory (memnest-learn)",
      `(snapshot:${reason} @ ${takenAt}; NOT new user input — call memory_search for the latest state)`,
      "<memnest_memory>",
      text,
      "</memnest_memory>",
    ].join("\n");
    return { systemPrompt: event.systemPrompt + header };
  });

  // agent_end: also capture what the ASSISTANT said, so failures/insights the
  // model discovered (but the user never restated) are visible to extraction.
  pi.on("agent_end", async (event: { messages?: unknown[]; willRetry?: boolean }) => {
    if (event?.willRetry) return; // partial turn that will be retried
    const msgs = Array.isArray(event?.messages) ? event.messages : [];
    for (const m of msgs) {
      if (!m || typeof m !== "object" || (m as any).role !== "assistant") continue;
      const text = extractMessageText((m as any).content);
      if (text.trim()) recentTurns.push({ role: "assistant", text: text.slice(0, 4000) });
    }
    if (recentTurns.length > MAX_RECENT_TURNS) recentTurns = recentTurns.slice(-MAX_RECENT_TURNS);
  });

  // input: track turns, correction fast-path, periodic background capture
  pi.on("input", async (event: { source?: string; text: string }, ctx: ExtensionContext) => {
    if (event.source === "extension") return { action: "continue" };
    const text = event.text ?? "";
    recentTurns.push({ role: "user", text });
    if (recentTurns.length > MAX_RECENT_TURNS) recentTurns = recentTurns.slice(-MAX_RECENT_TURNS);
    turnCounter++;

    const llm = backgroundLlm(ctx);
    const project = process.env.MEMNEST_PROJECT ?? "default";

    if (looksLikeCorrection(text)) {
      // store the correction immediately; mark snapshot dirty so it surfaces
      captureCorrection(text, client, project)
        .then(() => snapshot.markDirty())
        .catch(() => {});
    }

    // outcome reinforcement (#1): the closed loop. "still broken" raises the
    // matching failure memory; "works now" validates the one that helped.
    const signal = detectOutcomeSignal(text);
    if (signal) {
      reinforce(client, signal, text)
        .then((r) => {
          if (r.matched) snapshot.markDirty();
        })
        .catch(() => {});
    }

    if (llm && turnCounter % CAPTURE_EVERY_TURNS === 0 && recentTurns.length > 0 && !bgInFlight) {
      bgInFlight = true;
      const slice = recentTurns.slice(-40);
      captureMemories(slice, llm, client, { project, max: 8 })
        .then(async (r) => {
          if (r.written.length > 0) snapshot.markDirty();
          await learnFromMemories(r.memories, llm);
        })
        .catch(() => {})
        .finally(() => {
          bgInFlight = false;
        });
    }
    return { action: "continue" };
  });

  // session_before_compact: persist a handoff so in-progress context survives
  pi.on("session_before_compact", async (_event: unknown, ctx: ExtensionContext) => {
    ensureDirs();
    const sid = shortSid(ctx);
    const handoff = buildHandoff(readSafe(SCRATCHPAD_FILE), readSafe(dailyPath(today())), isoNow(), sid);
    if (handoff) {
      const fp = dailyPath(today());
      fs.writeFileSync(fp, appendDaily(readSafe(fp), handoff), "utf-8");
      client.summary(process.env.MEMNEST_PROJECT ?? "default", ctx.sessionManager.getSessionId(), handoff).catch(() => {});
    }
    // compaction is the one intentional cache boundary — refresh the snapshot
    await snapshot.refresh("before_compact", () => buildInjection(""));
  });

  // session_shutdown: final capture flush
  pi.on("session_shutdown", async (_event: unknown, ctx: ExtensionContext) => {
    const llm = backgroundLlm(ctx);
    if (!llm || recentTurns.length === 0) return;
    try {
      const r = await captureMemories(recentTurns.slice(-60), llm, client, {
        project: process.env.MEMNEST_PROJECT ?? "default",
        max: 12,
      });
      await learnFromMemories(r.memories, llm);
    } catch {
      /* best-effort */
    }
  });

  // tool: scratchpad checklist (working memory)
  const ScratchParams = Type.Object({
    action: Type.Union(
      [
        Type.Literal("add"),
        Type.Literal("done"),
        Type.Literal("undo"),
        Type.Literal("remove"),
        Type.Literal("list"),
        Type.Literal("clear"),
      ],
      { description: "Checklist action" },
    ),
    text: Type.Optional(
      Type.String({ description: "Item text (substring match for done/undo/remove)" }),
    ),
  });
  pi.registerTool({
    name: "scratchpad",
    label: "Scratchpad",
    description:
      "Manage a short-term checklist of things to do / remember this session (add, done, undo, remove, list, clear).",
    parameters: ScratchParams,
    async execute(_id, params, _signal, _onUpdate, _ctx) {
      ensureDirs();
      let items = parseScratchpad(readSafe(SCRATCHPAD_FILE));
      const { action, text } = params;
      if (action === "clear") {
        items = [];
      } else if (action !== "list") {
        if (!text) return toolText("text is required for this action");
        items = applyScratch(items, action, text);
      }
      if (action !== "list") fs.writeFileSync(SCRATCHPAD_FILE, formatScratchpad(items), "utf-8");
      const rendered = items.length
        ? items.map((i) => `- [${i.done ? "x" : " "}] ${i.text}`).join("\n")
        : "(empty)";
      return toolText(`Scratchpad:\n${rendered}`);
    },
  });

  // tool: skill (procedural "how", stored in memnest's _skills project)
  const SkillParams = Type.Object({
    action: Type.Union([Type.Literal("create"), Type.Literal("find"), Type.Literal("update")]),
    title: Type.Optional(Type.String()),
    body: Type.Optional(Type.String({ description: "Step-by-step procedure (create) or the step/caveat to append (update)" })),
    query: Type.Optional(Type.String({ description: "Search text (find) or which skill to refine (update)" })),
  });
  pi.registerTool({
    name: "skill",
    label: "Skill",
    description:
      "Save, recall, or refine a reusable procedure. create: save a how-to; find: search saved skills; update: append a learned step/caveat to the closest existing skill (self-improvement).",
    parameters: SkillParams,
    async execute(_id, params, _signal, _onUpdate, _ctx) {
      if (params.action === "create") {
        if (!params.title || !params.body) return toolText("title and body are required");
        const res = await client.add({
          text: `# ${params.title}\n${params.body}`,
          project: SKILL_PROJECT,
          category: "convention",
          importance: "knowledge",
          chunkType: "manual",
        });
        return toolText(`Saved skill "${params.title}" (id=${res.id}).`);
      }
      if (params.action === "update") {
        const needle = params.query ?? params.title ?? "";
        if (!needle || !params.body) return toolText("query (which skill) and body (what to add) are required");
        const hits = await client.search(needle, { project: SKILL_PROJECT, nResults: 1 });
        if (hits.length === 0) return toolText("No matching skill to update — use create instead.");
        const target = hits[0]!;
        const merged = `${target.document.trimEnd()}\n${params.body.trim()}`;
        await client.update(target.id, { text: merged, chunkType: "consolidated" });
        return toolText(`Refined skill (id=${target.id}).`);
      }
      const hits = await client.search(params.query ?? params.title ?? "", {
        project: SKILL_PROJECT,
        nResults: 5,
      });
      if (hits.length === 0) return toolText("No matching skills.");
      return toolText(hits.map((h, i) => `[${i + 1}] ${h.document.slice(0, 400)}`).join("\n\n"));
    },
  });

  // tool: manual consolidation trigger (embedding-based, dry-run by default)
  const ConsolidateParams = Type.Object({
    query: Type.String({ description: "Topic to consolidate around" }),
    apply: Type.Optional(Type.Boolean({ description: "Apply changes (default false = dry run)" })),
    maxDistance: Type.Optional(
      Type.Number({ description: "Cosine-distance cap for clustering (default 0.25)" }),
    ),
  });
  pi.registerTool({
    name: "memory_consolidate",
    label: "Consolidate memories",
    description: "Merge near-duplicate memories for a topic into one canonical entry (non-destructive).",
    parameters: ConsolidateParams,
    async execute(_id, params, _signal, _onUpdate, ctx) {
      const llm = makeLlm(ctx);
      if (!llm) return toolText("No active model for consolidation.");
      const items = await client.search(params.query, { nResults: 25 });
      const res = await consolidateByEmbedding(client, items, llm, {
        maxDistance: params.maxDistance ?? 0.25,
        apply: params.apply ?? false,
      });
      return toolText(
        `Consolidation (${params.apply ? "applied" : "dry-run"}): ${res.clusters} clusters, ${res.merged} merged, ${res.superseded} superseded.`,
      );
    },
  });
}

function toolText(text: string) {
  return { content: [{ type: "text" as const, text }], details: undefined };
}
