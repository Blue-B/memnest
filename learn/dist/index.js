// src/index.ts
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { complete } from "@earendil-works/pi-ai";
import { Type } from "@sinclair/typebox";

// src/memnest-client.ts
var SUPERSEDED_PROJECT = "_superseded";

class MemnestClient {
  baseUrl;
  fetchFn;
  constructor(baseUrl, fetchFn) {
    this.baseUrl = baseUrl;
    this.fetchFn = fetchFn;
  }
  async post(path, body, signal) {
    const res = await this.fetchFn(`${this.baseUrl}${path}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
      signal
    });
    if (!res.ok) {
      throw new Error(`memnest ${path} -> HTTP ${res.status}: ${await safeText(res)}`);
    }
    return res.json();
  }
  async add(input) {
    return this.post("/add", {
      text: input.text,
      project: input.project ?? "default",
      metadata: {
        chunk_type: input.chunkType ?? "manual",
        importance: input.importance ?? "knowledge",
        category: input.category ?? "general",
        sensitive: input.sensitive ?? false
      }
    });
  }
  async neighbors(opts) {
    const out = await this.post("/neighbors", {
      id: opts.id ?? "",
      text: opts.text ?? "",
      k: opts.k ?? 10,
      max_distance: opts.maxDistance ?? 0,
      project: opts.project ?? "all"
    });
    return out ?? [];
  }
  async search(query, opts = {}) {
    const body = {
      query,
      project: opts.project ?? "all",
      n_results: opts.nResults ?? 10,
      recent_first: opts.recentFirst ?? false
    };
    if (opts.category)
      body.category = opts.category;
    const out = await this.post("/search", body);
    return out?.results ?? [];
  }
  async context(query, opts = {}) {
    const out = await this.post("/context", {
      query,
      project: opts.project ?? "all",
      n_results: opts.nResults ?? 6,
      max_notes: opts.maxNotes ?? 12,
      max_facts: opts.maxFacts ?? 8,
      max_chars: opts.maxChars ?? 6000
    });
    return { prompt: String(out?.prompt ?? "") };
  }
  async update(id, fields) {
    const body = { id };
    if (fields.text !== undefined)
      body.text = fields.text;
    if (fields.project !== undefined)
      body.project = fields.project;
    if (fields.importance !== undefined)
      body.importance = fields.importance;
    if (fields.chunkType !== undefined)
      body.chunk_type = fields.chunkType;
    return this.post("/update", body);
  }
  async supersede(id) {
    return this.update(id, { project: SUPERSEDED_PROJECT, chunkType: "consolidated" });
  }
  async summary(project, sessionId, summary) {
    return this.post("/summary", { project, session_id: sessionId, summary });
  }
}
async function safeText(res) {
  try {
    return (await res.text()).slice(0, 200);
  } catch {
    return "<no body>";
  }
}

// src/kv-snapshot.ts
function emptySnapshot() {
  return { text: null, takenAt: null, takenOnDate: null, reason: null, dirty: false };
}
var systemClock = {
  isoNow: () => new Date().toISOString(),
  today: () => new Date().toISOString().slice(0, 10)
};

class MemorySnapshot {
  clock;
  state = emptySnapshot();
  constructor(clock = systemClock) {
    this.clock = clock;
  }
  async refresh(reason, builder) {
    const text = await builder();
    this.state = {
      text,
      takenAt: this.clock.isoNow(),
      takenOnDate: this.clock.today(),
      reason,
      dirty: false
    };
  }
  markDirty() {
    this.state.dirty = true;
  }
  needsRefresh() {
    return this.state.text === null || this.state.dirty || this.state.takenOnDate !== this.clock.today();
  }
  nextReason() {
    if (this.state.text === null)
      return "first_turn";
    if (this.state.dirty)
      return "long_term_write";
    return "day_rollover";
  }
  async get(builder) {
    if (this.needsRefresh()) {
      await this.refresh(this.nextReason(), builder);
    }
    return { text: this.state.text ?? "", reason: this.state.reason, takenAt: this.state.takenAt };
  }
  peek() {
    return { ...this.state };
  }
}

// src/types.ts
var MEMORY_CATEGORIES = [
  "general",
  "failure",
  "correction",
  "insight",
  "preference",
  "convention",
  "tool_quirk"
];
function defaultImportanceFor(category) {
  switch (category) {
    case "preference":
      return "preference";
    case "correction":
    case "convention":
      return "decision";
    case "failure":
    case "insight":
    case "tool_quirk":
      return "knowledge";
    default:
      return "log";
  }
}

// src/extract.ts
var EXTRACTION_SYSTEM_PROMPT = [
  "You extract durable, reusable memories from a coding-assistant conversation.",
  "Return ONLY a JSON array (no prose, no code fence). Each element:",
  '{ "category": one of [' + MEMORY_CATEGORIES.join(", ") + '], "text": string, "importance"?: "log"|"knowledge"|"decision"|"preference" }',
  "Rules:",
  "- Save only things worth recalling in a FUTURE session: failures + their cause, user corrections, durable preferences, project conventions, tool quirks, hard-won insights.",
  "- Do NOT save transient chatter, restated file contents, or anything trivially re-derivable.",
  "- Each text must be ONE self-contained sentence, understandable with no other context.",
  "- Never include secrets/credentials.",
  "- If nothing is worth saving, return []."
].join(`
`);
function buildExtractionUserPrompt(turns, maxChars = 12000) {
  let text = turns.filter((t) => t.role !== "system").map((t) => `${t.role.toUpperCase()}: ${t.text}`).join(`
`);
  if (text.length > maxChars)
    text = text.slice(-maxChars);
  return `Conversation:
${text}

Return the JSON array of memories to save.`;
}
function extractJsonArray(raw) {
  const fenced = raw.replace(/```(?:json)?/gi, "");
  const start = fenced.indexOf("[");
  if (start === -1)
    return [];
  let depth = 0;
  for (let i = start;i < fenced.length; i++) {
    const ch = fenced[i];
    if (ch === "[")
      depth++;
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
function isCategory(v) {
  return typeof v === "string" && MEMORY_CATEGORIES.includes(v);
}
var IMPORTANCES = ["log", "knowledge", "decision", "preference"];
function parseExtraction(raw) {
  const arr = extractJsonArray(raw);
  if (!Array.isArray(arr))
    return [];
  const out = [];
  for (const el of arr) {
    if (!el || typeof el !== "object")
      continue;
    const obj = el;
    const text = typeof obj.text === "string" ? obj.text.trim() : "";
    if (!text)
      continue;
    const category = isCategory(obj.category) ? obj.category : "general";
    const importance = typeof obj.importance === "string" && IMPORTANCES.includes(obj.importance) ? obj.importance : undefined;
    out.push({ category, text, importance });
  }
  return out;
}
async function extractMemories(turns, llm) {
  if (turns.length === 0)
    return [];
  const reply = await llm({
    system: EXTRACTION_SYSTEM_PROMPT,
    user: buildExtractionUserPrompt(turns)
  });
  return parseExtraction(reply);
}

// src/capture.ts
async function captureMemories(turns, llm, client, opts = {}) {
  const memories = await extractMemories(turns, llm);
  const limited = opts.max ? memories.slice(0, opts.max) : memories;
  const seen = new Set;
  const written = [];
  const persisted = [];
  let errors = 0;
  for (const m of limited) {
    const key = m.text.trim().toLowerCase();
    if (seen.has(key))
      continue;
    seen.add(key);
    try {
      const res = await client.add({
        text: m.text,
        project: opts.project ?? "default",
        category: m.category,
        importance: m.importance ?? defaultImportanceFor(m.category),
        chunkType: "manual"
      });
      if (res?.id)
        written.push(res.id);
      persisted.push(m);
    } catch {
      errors++;
    }
  }
  return { extracted: memories.length, written, errors, memories: persisted };
}
var CORRECTION_SYSTEM_PROMPT = [
  "You convert a user's in-the-moment correction of a coding assistant into ONE",
  "durable lesson for FUTURE sessions. The user's words are a complaint; the",
  "actual lesson lives in what the assistant just did wrong.",
  "Output ONLY the lesson — one self-contained sentence, no quotes, no prose,",
  "no markdown. Write it in the SAME language the user used.",
  "It must state BOTH what the assistant did wrong AND what to do instead,",
  "inferred from the conversation (e.g. 'don't assume the network is WiFi —",
  "verify with Get-NetAdapter'). Make it a concrete rule, not a restatement of",
  "the complaint. If you cannot infer a concrete lesson, output exactly: NONE"
].join(`
`);
async function extractCorrectionLesson(correctionText, context, llm) {
  const convo = context.filter((t) => t.role !== "system").slice(-8).map((t) => `${t.role.toUpperCase()}: ${t.text.slice(0, 1500)}`).join(`
`);
  const reply = (await llm({
    system: CORRECTION_SYSTEM_PROMPT,
    user: `Conversation:
${convo}

The user's correction: ${correctionText}

Return the one-sentence lesson.`
  })).trim().replace(/^["'`]+|["'`]+$/g, "").trim();
  if (!reply || reply === "NONE" || /^none$/i.test(reply))
    return null;
  return reply.slice(0, 500);
}
async function captureCorrection(correctionText, client, project = "default", opts = {}) {
  const raw = correctionText.trim();
  if (!raw)
    return null;
  let lesson = raw;
  let distilled = false;
  if (opts.llm && opts.context && opts.context.length > 0) {
    try {
      const extracted = await extractCorrectionLesson(raw, opts.context, opts.llm);
      if (extracted) {
        lesson = extracted;
        distilled = true;
      }
    } catch {}
  }
  const res = await client.add({
    text: lesson,
    project,
    category: "correction",
    importance: distilled ? "preference" : "decision",
    chunkType: "manual"
  });
  return { id: res?.id ?? null, lesson, distilled };
}
function extractMessageText(content) {
  if (typeof content === "string")
    return content;
  if (!Array.isArray(content))
    return "";
  const parts = [];
  for (const block of content) {
    if (typeof block === "string")
      parts.push(block);
    else if (block && typeof block === "object" && block.type === "text" && typeof block.text === "string") {
      parts.push(block.text);
    }
  }
  return parts.join(`
`).trim();
}
var CORRECTION_PATTERNS = [
  /\bno,? (use|don't|do not|that's wrong|not)\b/i,
  /\bactually,?\b/i,
  /\bthat'?s (wrong|incorrect|not right)\b/i,
  /\b(use|prefer) .* not\b/i,
  /아니(야|요|라|\s|$)/,
  /틀렸/,
  /잘못/,
  /추정/,
  /말고/,
  /없는데/,
  /했잖아/,
  /잖아(요)?/,
  /(해야|말아|하면).*(잖|는데)/,
  /그럴\s*(것|거|꺼)?\s*같/,
  /(안 ?될|안 ?돼)\s*(것|거|꺼)?\s*같/,
  /또\s*(실패|안돼|망|똑같)/,
  /의미\s*없/
];
function looksLikeCorrection(userText) {
  const t = userText.trim();
  if (!t)
    return false;
  return CORRECTION_PATTERNS.some((re) => re.test(t));
}

// src/consolidate.ts
function trigrams(s) {
  const norm = s.toLowerCase().replace(/\s+/g, " ").trim();
  if (norm.length < 3)
    return new Set([norm]);
  const set = new Set;
  for (let i = 0;i <= norm.length - 3; i++)
    set.add(norm.slice(i, i + 3));
  return set;
}
function trigramSimilarity(a, b) {
  const ta = trigrams(a);
  const tb = trigrams(b);
  if (ta.size === 0 && tb.size === 0)
    return 1;
  let inter = 0;
  for (const t of ta)
    if (tb.has(t))
      inter++;
  const union = ta.size + tb.size - inter;
  return union === 0 ? 0 : inter / union;
}
var MERGE_SYSTEM_PROMPT = [
  "You merge several near-duplicate memory entries into ONE.",
  "Return ONLY the merged memory text (no prose, no quotes, no code fence).",
  "Preserve every distinct fact across the inputs; drop pure repetition.",
  "Keep it concise and self-contained."
].join(`
`);
async function planClusterMerge(cluster, llm) {
  if (cluster.length < 2)
    return null;
  const keep = cluster[0];
  const merged = (await llm({
    system: MERGE_SYSTEM_PROMPT,
    user: cluster.map((c, i) => `(${i + 1}) ${c.document}`).join(`
`)
  })).trim();
  if (!merged)
    return null;
  return {
    keepId: keep.id,
    mergedText: merged,
    supersededIds: cluster.slice(1).map((c) => c.id)
  };
}
async function consolidateByEmbedding(client, items, llm, opts = {}) {
  const maxDistance = opts.maxDistance ?? 0.25;
  const apply = opts.apply ?? true;
  const byId = new Map(items.map((i) => [i.id, i]));
  const parent = new Map;
  for (const i of items)
    parent.set(i.id, i.id);
  const find = (x) => {
    let r = x;
    while (parent.get(r) !== r)
      r = parent.get(r);
    let c = x;
    while (parent.get(c) !== r) {
      const next = parent.get(c);
      parent.set(c, r);
      c = next;
    }
    return r;
  };
  const union = (a, b) => {
    const ra = find(a);
    const rb = find(b);
    if (ra !== rb)
      parent.set(ra, rb);
  };
  for (const it of items) {
    const ns = await client.neighbors({ id: it.id, maxDistance, k: 20 });
    for (const n of ns)
      if (byId.has(n.id))
        union(it.id, n.id);
  }
  const groups = new Map;
  for (const it of items) {
    const root = find(it.id);
    const g = groups.get(root) ?? [];
    g.push(it);
    groups.set(root, g);
  }
  const clusters = [...groups.values()].filter((c) => c.length >= 2).map((c) => c.slice().sort((a, b) => b.score - a.score));
  let merged = 0;
  let superseded = 0;
  for (const cluster of clusters) {
    const plan = await planClusterMerge(cluster, llm);
    if (!plan)
      continue;
    if (apply) {
      await client.update(plan.keepId, { text: plan.mergedText, chunkType: "consolidated" });
      for (const id of plan.supersededIds)
        await client.supersede(id);
    }
    merged++;
    superseded += plan.supersededIds.length;
  }
  return { clusters: clusters.length, merged, superseded };
}

// src/budget.ts
class LlmBudget {
  maxCalls;
  windowMs;
  now;
  times = [];
  constructor(maxCalls, windowMs, now = Date.now) {
    this.maxCalls = maxCalls;
    this.windowMs = windowMs;
    this.now = now;
  }
  allow() {
    if (this.maxCalls <= 0)
      return false;
    const t = this.now();
    const cutoff = t - this.windowMs;
    this.times = this.times.filter((x) => x > cutoff);
    if (this.times.length >= this.maxCalls)
      return false;
    this.times.push(t);
    return true;
  }
  state() {
    const cutoff = this.now() - this.windowMs;
    const used = this.times.filter((x) => x > cutoff).length;
    return { used, max: this.maxCalls, windowMs: this.windowMs };
  }
}

// src/reinforce.ts
var LADDER = ["log", "knowledge", "decision", "preference"];
function bumpImportance(cur, cap = "preference") {
  const i = LADDER.indexOf(cur.toLowerCase());
  const capIdx = LADDER.indexOf(cap);
  if (i < 0)
    return "knowledge";
  return LADDER[Math.min(i + 1, capIdx)];
}
var MARKER_RE = /\s*\[recurred [×x](\d+)\]\s*$/i;
function recurrenceCount(text) {
  const m = MARKER_RE.exec(text);
  return m ? Number(m[1]) : 0;
}
function withRecurrenceMarker(text, n) {
  return `${text.replace(MARKER_RE, "").trimEnd()} [recurred ×${n}]`;
}
var RECURRENCE_PATTERNS = [
  /still\b[^.!?]*\b(broken|not working|fails?|failing|happening|the same|dying|crashing|crashes|down|there|here|wrong|bad)/i,
  /same (error|issue|problem|bug|thing)( again)?/i,
  /(did|does|doesn'?t|didn'?t) ?n'?t? (work|fix)/i,
  /not fixed/i,
  /\b(is|it'?s|it is|are|they'?re|thing'?s|that'?s) back\b/i,
  /\bback again\b/i,
  /\b(happening|crashing|dying|failing|broke|broken|error) again\b/i,
  /keeps? (happening|coming back|dying|crashing|failing|breaking)/i,
  /여전히/,
  /아직(도| 안|$)/,
  /또 (안|같은|터|죽|에러|발생|그)/,
  /다시[^.!?]*(죽|안|에러|터|발생)/,
  /재발/,
  /안 ?(고쳐|됐|돼|되|풀렸)/,
  /그대로(네|야|다|임|\s|$)/,
  /똑같(이|은|네|아)/
];
var SUCCESS_PATTERNS = [
  /(works|working|fine) now/i,
  /(that|it) (works|worked|fixed it|did it)/i,
  /\bfixed( it)?\b/i,
  /됐(어|다|네|음)/,
  /(잘|이제) ?(돼|된다|됨|작동)/,
  /고쳐졌/,
  /해결(됐|했|돼|된)/,
  /성공(했|이|\s|$)/
];
function detectOutcomeSignal(text) {
  const t = text.trim();
  if (!t)
    return null;
  if (RECURRENCE_PATTERNS.some((re) => re.test(t)))
    return "recurrence";
  if (SUCCESS_PATTERNS.some((re) => re.test(t)))
    return "success";
  return null;
}
var NEIGHBOR_DOC_LIMIT = 8000;
var RECUR_CATS = new Set(["failure", "correction", "tool_quirk"]);
var SUCCESS_CATS = new Set(["failure", "correction", "insight", "tool_quirk", "convention"]);
async function reinforce(client, signal, contextText, opts = {}) {
  if (!signal)
    return { matched: false };
  const text = contextText.trim();
  if (!text)
    return { matched: false };
  const maxDistance = opts.maxDistance ?? (signal === "recurrence" ? 0.22 : 0.2);
  const ns = await client.neighbors({
    text,
    k: opts.k ?? 8,
    maxDistance,
    project: opts.project ?? "all"
  });
  if (ns.length === 0)
    return { matched: false };
  const want = signal === "recurrence" ? RECUR_CATS : SUCCESS_CATS;
  const hit = ns.find((n) => want.has((n.category || "").toLowerCase()));
  if (!hit)
    return { matched: false };
  if (signal === "recurrence") {
    const newImportance2 = bumpImportance(hit.importance, "decision");
    if (hit.document.length >= NEIGHBOR_DOC_LIMIT) {
      await client.update(hit.id, { importance: newImportance2 });
      return { matched: true, id: hit.id, action: "reinforced", newImportance: newImportance2 };
    }
    const n = recurrenceCount(hit.document) + 1;
    await client.update(hit.id, {
      text: withRecurrenceMarker(hit.document, n),
      importance: newImportance2
    });
    return { matched: true, id: hit.id, action: "reinforced", newImportance: newImportance2, recurred: n };
  }
  const newImportance = bumpImportance(hit.importance, "decision");
  if (newImportance !== (hit.importance || "").toLowerCase()) {
    await client.update(hit.id, { importance: newImportance });
  }
  return { matched: true, id: hit.id, action: "validated", newImportance };
}

// src/skills.ts
var SKILL_PROJECT = "_skills";
var PROCEDURAL_CATS = new Set([
  "convention",
  "insight",
  "tool_quirk",
  "correction"
]);
function isSkillCandidate(m) {
  return PROCEDURAL_CATS.has(m.category);
}
var SKILL_REFINE_SYSTEM_PROMPT = [
  "You maintain a reusable skill (a how-to procedure) for a coding agent.",
  "Given the CURRENT skill and a NEW learning, return ONLY the improved skill text",
  "(no prose, no quotes, no code fence).",
  "Integrate the new learning as a step or caveat. Preserve every existing step.",
  "Keep it concise, ordered, and self-contained. Keep the leading '# Title' line."
].join(`
`);
var SKILL_DRAFT_SYSTEM_PROMPT = [
  "You write a short reusable skill (how-to) for a coding agent from one learning.",
  "If the learning is a genuine reusable procedure, return:",
  "  a single '# Title' line, then 2-6 numbered steps.",
  "If it is NOT a reusable procedure (just a fact/preference), return exactly: NONE",
  "Return ONLY that — no prose, no quotes, no code fence."
].join(`
`);
async function improveSkills(client, candidates, llm, opts = {}) {
  const maxDistance = opts.maxDistance ?? 0.32;
  const apply = opts.apply ?? true;
  const pool = candidates.filter(isSkillCandidate).slice(0, opts.max ?? 6);
  let improved = 0;
  let created = 0;
  for (const m of pool) {
    const ns = await client.neighbors({
      text: m.text,
      project: SKILL_PROJECT,
      k: 3,
      maxDistance
    });
    if (ns.length > 0) {
      const target = ns[0];
      const refined = (await llm({
        system: SKILL_REFINE_SYSTEM_PROMPT,
        user: `CURRENT SKILL:
${target.document}

NEW LEARNING:
${m.text}`
      })).trim();
      if (refined && refined.toUpperCase() !== "NONE") {
        if (apply)
          await client.update(target.id, { text: refined, chunkType: "consolidated" });
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
            chunkType: "manual"
          });
        created++;
      }
    }
  }
  return { improved, created };
}

// src/user-model.ts
var USER_MODEL_PROJECT = "_user_model";
var FACET_CATS = new Set(["preference"]);
function isUserFacet(m) {
  return FACET_CATS.has(m.category);
}
var USER_MODEL_REFINE_SYSTEM_PROMPT = [
  "You maintain a concise, evolving model of one specific user — their",
  "preferences, conventions, and working style.",
  "Given an EXISTING facet and a NEW observation about the same aspect, return",
  "ONLY one improved facet sentence (no prose, no quotes, no code fence).",
  "Make it sharper and current; if the two conflict, prefer the NEW observation."
].join(`
`);
async function updateUserModel(client, facts, llm, opts = {}) {
  const maxDistance = opts.maxDistance ?? 0.22;
  const apply = opts.apply ?? true;
  const pool = facts.filter(isUserFacet).slice(0, opts.max ?? 6);
  let refined = 0;
  let added = 0;
  const seenThisBatch = [];
  for (const f of pool) {
    if (seenThisBatch.some((t) => trigramSimilarity(t, f.text) >= 0.5))
      continue;
    seenThisBatch.push(f.text);
    const ns = await client.neighbors({
      text: f.text,
      project: USER_MODEL_PROJECT,
      k: 3,
      maxDistance
    });
    if (ns.length > 0) {
      const target = ns[0];
      const merged = (await llm({
        system: USER_MODEL_REFINE_SYSTEM_PROMPT,
        user: `EXISTING:
${target.document}

NEW:
${f.text}`
      })).trim();
      if (merged) {
        if (apply)
          await client.update(target.id, { text: merged, importance: "preference" });
        refined++;
      }
    } else {
      if (apply)
        await client.add({
          text: f.text,
          project: USER_MODEL_PROJECT,
          category: f.category,
          importance: "preference",
          chunkType: "manual"
        });
      added++;
    }
  }
  return { refined, added };
}
async function userModelContext(client, query, opts = {}) {
  const max = opts.max ?? 5;
  const hits = await client.search(query || "user preferences and working style", {
    project: USER_MODEL_PROJECT,
    nResults: max
  });
  if (hits.length === 0)
    return "";
  const lines = hits.map((h) => `- ${h.document.replace(/\s+/g, " ").trim()}`);
  return ["user_profile:", ...lines].join(`
`);
}

// src/working-memory.ts
var ITEM_RE = /^- \[( |x|X)\]\s+(.*)$/;
function parseScratchpad(content) {
  const items = [];
  for (const line of content.split(`
`)) {
    const m = ITEM_RE.exec(line.trim());
    if (m)
      items.push({ done: m[1].toLowerCase() === "x", text: m[2].trim() });
  }
  return items;
}
function formatScratchpad(items) {
  if (items.length === 0)
    return `# Scratchpad
`;
  const lines = items.map((i) => `- [${i.done ? "x" : " "}] ${i.text}`);
  return `# Scratchpad
${lines.join(`
`)}
`;
}
function applyScratch(items, action, text) {
  const needle = text.trim().toLowerCase();
  if (action === "add") {
    if (items.some((i) => i.text.toLowerCase() === needle))
      return items;
    return [...items, { text: text.trim(), done: false }];
  }
  if (action === "remove") {
    return items.filter((i) => !i.text.toLowerCase().includes(needle));
  }
  const done = action === "done";
  return items.map((i) => i.text.toLowerCase().includes(needle) ? { ...i, done } : i);
}
function appendDaily(existing, entry) {
  const sep = existing.trim() ? `

` : "";
  return existing + sep + entry;
}
function buildHandoff(scratchpad, todayLog, isoTs, shortSid, tailLines = 15) {
  const parts = [];
  const open = parseScratchpad(scratchpad).filter((i) => !i.done);
  if (open.length > 0) {
    parts.push("**Open scratchpad items:**");
    for (const i of open)
      parts.push(`- [ ] ${i.text}`);
  }
  const log = todayLog.trim();
  if (log) {
    const tail = log.split(`
`).slice(-tailLines).join(`
`);
    parts.push(`**Recent daily log:**
${tail}`);
  }
  if (parts.length === 0)
    return null;
  return [`<!-- HANDOFF ${isoTs} [${shortSid}] -->`, "## Session Handoff", ...parts].join(`
`);
}

// src/index.ts
var MEMNEST_URL = process.env.MEMNEST_URL ?? "http://127.0.0.1:3111";
var LEARN_DIR = process.env.MEMNEST_LEARN_DIR ?? path.join(os.homedir(), ".pi", "agent", "memnest-learn");
var SCRATCHPAD_FILE = path.join(LEARN_DIR, "SCRATCHPAD.md");
var DAILY_DIR = path.join(LEARN_DIR, "daily");
var CAPTURE_EVERY_TURNS = Number(process.env.MEMNEST_CAPTURE_TURNS ?? 10);
var LLM_MAX_CALLS = Number(process.env.MEMNEST_LLM_MAX_CALLS ?? 24);
var LLM_WINDOW_MS = Number(process.env.MEMNEST_LLM_WINDOW_MS ?? 5 * 60 * 1000);
var client = new MemnestClient(MEMNEST_URL, fetch);
var snapshot = new MemorySnapshot;
var budget = new LlmBudget(LLM_MAX_CALLS, LLM_WINDOW_MS);
var turnCounter = 0;
var recentTurns = [];
var bgInFlight = false;
var MAX_RECENT_TURNS = 120;
function ensureDirs() {
  fs.mkdirSync(DAILY_DIR, { recursive: true });
}
function readSafe(p) {
  try {
    return fs.readFileSync(p, "utf-8");
  } catch {
    return "";
  }
}
var today = () => new Date().toISOString().slice(0, 10);
var isoNow = () => new Date().toISOString();
function currentProject() {
  const env = process.env.MEMNEST_PROJECT?.trim();
  if (env)
    return env;
  const base = path.basename(process.cwd());
  return base && base !== "/" ? base : "default";
}
var warn = (e) => console.warn("[memnest-learn]", e instanceof Error ? e.message : String(e));
var dailyPath = (d) => path.join(DAILY_DIR, `${d}.md`);
var shortSid = (ctx) => ctx.sessionManager.getSessionId().slice(0, 8);
function makeLlm(ctx) {
  if (!ctx.model)
    return null;
  return async ({ system, user }) => {
    const res = await complete(ctx.model, { systemPrompt: system, messages: [{ role: "user", content: [{ type: "text", text: user }], timestamp: Date.now() }] }, { reasoningEffort: "low" });
    return res.content.filter((c) => c.type === "text" && typeof c.text === "string").map((c) => c.text).join(`
`).trim();
  };
}
function backgroundLlm(ctx) {
  const llm = makeLlm(ctx);
  if (!llm)
    return null;
  return async (input) => budget.allow() ? llm(input) : "";
}
async function learnFromMemories(memories, llm) {
  if (memories.length === 0)
    return;
  await Promise.allSettled([
    improveSkills(client, memories, llm, { max: 4 }),
    updateUserModel(client, memories, llm, { max: 4 })
  ]);
}
async function buildInjection(prompt) {
  const parts = [];
  try {
    const profile = await userModelContext(client, prompt || "user preferences working style");
    if (profile.trim())
      parts.push(profile);
  } catch (e) {
    warn(e);
  }
  const open = parseScratchpad(readSafe(SCRATCHPAD_FILE)).filter((i) => !i.done);
  if (open.length > 0) {
    parts.push("open_tasks:");
    for (const i of open)
      parts.push(`- [ ] ${i.text}`);
  }
  try {
    const rules = await client.search(prompt || "user corrections and preferences", {
      project: "playbook",
      nResults: 6
    });
    if (rules.length > 0) {
      parts.push("learned_rules (you were corrected on these before — follow them):");
      for (const r of rules) {
        parts.push(`- ${r.document.replace(/\s+/g, " ").trim().slice(0, 300)}`);
      }
    }
  } catch (e) {
    warn(e);
  }
  try {
    const { prompt: pack } = await client.context(prompt || "recent work", { maxChars: 3000 });
    if (pack.trim())
      parts.push(pack);
  } catch (e) {
    warn(e);
  }
  return parts.join(`
`);
}
function src_default(pi) {
  pi.on("session_start", async () => {
    ensureDirs();
    recentTurns = [];
    turnCounter = 0;
    await snapshot.refresh("session_start", () => buildInjection(""));
  });
  pi.on("before_agent_start", async (event) => {
    const { text, reason, takenAt } = await snapshot.get(() => buildInjection(event.prompt ?? ""));
    if (!text.trim())
      return;
    const header = [
      `

## Memory (memnest-learn)`,
      `(snapshot:${reason} @ ${takenAt}; NOT new user input — call memory_search for the latest state)`,
      "<memnest_memory>",
      text,
      "</memnest_memory>"
    ].join(`
`);
    return { systemPrompt: event.systemPrompt + header };
  });
  pi.on("agent_end", async (event) => {
    if (event?.willRetry)
      return;
    const msgs = Array.isArray(event?.messages) ? event.messages : [];
    for (const m of msgs) {
      if (!m || typeof m !== "object" || m.role !== "assistant")
        continue;
      const text = extractMessageText(m.content);
      if (text.trim())
        recentTurns.push({ role: "assistant", text: text.slice(0, 4000) });
    }
    if (recentTurns.length > MAX_RECENT_TURNS)
      recentTurns = recentTurns.slice(-MAX_RECENT_TURNS);
  });
  pi.on("input", async (event, ctx) => {
    if (event.source === "extension")
      return { action: "continue" };
    const text = event.text ?? "";
    recentTurns.push({ role: "user", text });
    if (recentTurns.length > MAX_RECENT_TURNS)
      recentTurns = recentTurns.slice(-MAX_RECENT_TURNS);
    turnCounter++;
    const llm = backgroundLlm(ctx);
    const project = currentProject();
    if (looksLikeCorrection(text)) {
      captureCorrection(text, client, project, { llm, context: recentTurns }).then(async (r) => {
        if (!r)
          return;
        snapshot.markDirty();
        const tag = r.distilled ? "\uD83E\uDDE0 교정 학습" : "\uD83D\uDCDD 교정 기록";
        const short = r.lesson.length > 70 ? r.lesson.slice(0, 67) + "..." : r.lesson;
        // notify 제거: 채팅 밀림 원인. setStatus만 유지.
        ctx.ui.setStatus("memnest-correction", `${tag}: ${short}`);
        if (r.distilled && llm) {
          await updateUserModel(client, [{ category: "preference", text: r.lesson }], llm, { max: 1 }).catch(warn);
        }
      }).catch(warn);
    }
    const signal = detectOutcomeSignal(text);
    if (signal) {
      const contextText = [...recentTurns.slice(-3).map((t) => t.text), text].join(`
`);
      reinforce(client, signal, contextText).then((r) => {
        if (r.matched)
          snapshot.markDirty();
      }).catch(warn);
    }
    if (llm && turnCounter % CAPTURE_EVERY_TURNS === 0 && recentTurns.length > 0 && !bgInFlight) {
      bgInFlight = true;
      const slice = recentTurns.slice(-40);
      captureMemories(slice, llm, client, { project, max: 8 }).then(async (r) => {
        if (r.written.length > 0)
          snapshot.markDirty();
        await learnFromMemories(r.memories, llm);
      }).catch(warn).finally(() => {
        bgInFlight = false;
      });
    }
    return { action: "continue" };
  });
  pi.on("session_before_compact", async (_event, ctx) => {
    ensureDirs();
    const sid = shortSid(ctx);
    const handoff = buildHandoff(readSafe(SCRATCHPAD_FILE), readSafe(dailyPath(today())), isoNow(), sid);
    if (handoff) {
      const fp = dailyPath(today());
      fs.writeFileSync(fp, appendDaily(readSafe(fp), handoff), "utf-8");
      client.summary(currentProject(), ctx.sessionManager.getSessionId(), handoff).catch(warn);
    }
    await snapshot.refresh("before_compact", () => buildInjection(""));
  });
  pi.on("session_shutdown", async (_event, ctx) => {
    const llm = backgroundLlm(ctx);
    if (!llm || recentTurns.length === 0)
      return;
    try {
      const r = await captureMemories(recentTurns.slice(-60), llm, client, {
        project: currentProject(),
        max: 12
      });
      await learnFromMemories(r.memories, llm);
    } catch (e) {
      warn(e);
    }
  });
  const ScratchParams = Type.Object({
    action: Type.Union([
      Type.Literal("add"),
      Type.Literal("done"),
      Type.Literal("undo"),
      Type.Literal("remove"),
      Type.Literal("list"),
      Type.Literal("clear")
    ], { description: "Checklist action" }),
    text: Type.Optional(Type.String({ description: "Item text (substring match for done/undo/remove)" }))
  });
  pi.registerTool({
    name: "scratchpad",
    label: "Scratchpad",
    description: "Manage a short-term checklist of things to do / remember this session (add, done, undo, remove, list, clear).",
    parameters: ScratchParams,
    async execute(_id, params, _signal, _onUpdate, _ctx) {
      ensureDirs();
      let items = parseScratchpad(readSafe(SCRATCHPAD_FILE));
      const { action, text } = params;
      if (action === "clear") {
        items = [];
      } else if (action !== "list") {
        if (!text)
          return toolText("text is required for this action");
        items = applyScratch(items, action, text);
      }
      if (action !== "list")
        fs.writeFileSync(SCRATCHPAD_FILE, formatScratchpad(items), "utf-8");
      const rendered = items.length ? items.map((i) => `- [${i.done ? "x" : " "}] ${i.text}`).join(`
`) : "(empty)";
      return toolText(`Scratchpad:
${rendered}`);
    }
  });
  const SkillParams = Type.Object({
    action: Type.Union([Type.Literal("create"), Type.Literal("find"), Type.Literal("update")]),
    title: Type.Optional(Type.String()),
    body: Type.Optional(Type.String({ description: "Step-by-step procedure (create) or the step/caveat to append (update)" })),
    query: Type.Optional(Type.String({ description: "Search text (find) or which skill to refine (update)" }))
  });
  pi.registerTool({
    name: "skill",
    label: "Skill",
    description: "Save, recall, or refine a reusable procedure. create: save a how-to; find: search saved skills; update: append a learned step/caveat to the closest existing skill (self-improvement).",
    parameters: SkillParams,
    async execute(_id, params, _signal, _onUpdate, _ctx) {
      if (params.action === "create") {
        if (!params.title || !params.body)
          return toolText("title and body are required");
        const res = await client.add({
          text: `# ${params.title}
${params.body}`,
          project: SKILL_PROJECT,
          category: "convention",
          importance: "knowledge",
          chunkType: "manual"
        });
        return toolText(`Saved skill "${params.title}" (id=${res.id}).`);
      }
      if (params.action === "update") {
        const needle = params.query ?? params.title ?? "";
        if (!needle || !params.body)
          return toolText("query (which skill) and body (what to add) are required");
        const hits2 = await client.search(needle, { project: SKILL_PROJECT, nResults: 1 });
        if (hits2.length === 0)
          return toolText("No matching skill to update — use create instead.");
        const target = hits2[0];
        const merged = `${target.document.trimEnd()}
${params.body.trim()}`;
        await client.update(target.id, { text: merged, chunkType: "consolidated" });
        return toolText(`Refined skill (id=${target.id}).`);
      }
      const hits = await client.search(params.query ?? params.title ?? "", {
        project: SKILL_PROJECT,
        nResults: 5
      });
      if (hits.length === 0)
        return toolText("No matching skills.");
      return toolText(hits.map((h, i) => `[${i + 1}] ${h.document.slice(0, 400)}`).join(`

`));
    }
  });
  const ConsolidateParams = Type.Object({
    query: Type.String({ description: "Topic to consolidate around" }),
    apply: Type.Optional(Type.Boolean({ description: "Apply changes (default false = dry run)" })),
    maxDistance: Type.Optional(Type.Number({ description: "Cosine-distance cap for clustering (default 0.25)" }))
  });
  pi.registerTool({
    name: "memory_consolidate",
    label: "Consolidate memories",
    description: "Merge near-duplicate memories for a topic into one canonical entry (non-destructive).",
    parameters: ConsolidateParams,
    async execute(_id, params, _signal, _onUpdate, ctx) {
      const llm = makeLlm(ctx);
      if (!llm)
        return toolText("No active model for consolidation.");
      const items = await client.search(params.query, { nResults: 25 });
      const res = await consolidateByEmbedding(client, items, llm, {
        maxDistance: params.maxDistance ?? 0.25,
        apply: params.apply ?? false
      });
      return toolText(`Consolidation (${params.apply ? "applied" : "dry-run"}): ${res.clusters} clusters, ${res.merged} merged, ${res.superseded} superseded.`);
    }
  });
}
function toolText(text) {
  return { content: [{ type: "text", text }], details: undefined };
}
export {
  src_default as default
};
