#!/usr/bin/env node
// Task cards: shared collection, save replaces the same title, opened cards (up to three)
// are pinned on every run, a new session lists active cards ranked by the first request,
// done moves a card to the finished collection and save reopens it, user messages are kept
// verbatim and cannot be edited by the model, compaction triggers a save reminder, a resumed session reopens its
// cards, and a stalled core does not hold up the prompt.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const bundle = join(here, "..", "dist", "index.mjs");

const docs = [];
const adds = [];
const searches = [];
const updates = [];
globalThis.fetch = async (url, init) => {
	const path = new URL(url).pathname;
	const body = init?.body ? JSON.parse(init.body) : undefined;
	const json = (value) => new Response(JSON.stringify(value), { status: 200 });
	if (path === "/add") {
		adds.push(body);
		const old = docs.find((d) => d.id === body.metadata.supersedes);
		if (body.metadata.supersedes) assert.equal(old?.project, body.project, "core only supersedes within one project");
		if (old) old.project = "_superseded";
		docs.push({ id: `c${docs.length + 1}`, project: body.project, document: body.text, timestamp: `2026-11-${String(docs.length + 1).padStart(2, "0")}T00:00:00Z` });
		return json({ status: "succeeded", id: docs.at(-1).id });
	}
	if (path === "/search") {
		searches.push(body);
		assert.ok(["task-cards", "task-cards-done"].includes(body.project), "cards live in shared collections");
		let live = docs.filter((d) => d.project === body.project).reverse();
		// Fake relevance: documents sharing a word with the request come first.
		const words = body.query.split(/\s+/).slice(1).filter(Boolean);
		if (words.length) live = [...live.filter((d) => words.some((w) => d.document.includes(w))), ...live.filter((d) => !words.some((w) => d.document.includes(w)))];
		return json({ project: body.project, results: [{ id: "x", document: "unrelated", project: body.project, score: 1 }, ...live.map((d) => ({ ...d, document: d.document.slice(0, 600), score: 1 }))] });
	}
	if (path === "/update") {
		updates.push(body);
		const doc = docs.find((d) => d.id === body.id);
		if (body.project) doc.project = body.project;
		if (body.text) doc.document = body.text;
		return json({ status: "ok" });
	}
	if (path.startsWith("/chunk/")) {
		const d = docs.find((x) => x.id === decodeURIComponent(path.slice(7)));
		return json({ id: d.id, document: d.document });
	}
	throw new Error(`unexpected ${path}`);
};

const tools = new Map();
const hooks = new Map();
const entries = [];
(await import(bundle)).default({
	registerTool: (t) => tools.set(t.name, t),
	registerCommand() {},
	on: (name, fn) => { if (!hooks.has(name)) hooks.set(name, fn); },
	appendEntry: (customType, data) => entries.push({ type: "custom", customType, data }),
});
const tool = tools.get("task_card");
const call = async (args) => (await tool.execute("1", args, null, null, { cwd: "/somewhere" })).content[0].text;
let branch = [];
const start = (entries = []) => { branch = entries; hooks.get("session_start")({}, { sessionManager: { getBranch: () => branch } }); };
const run = (prompt = "hi", options = { contextFiles: [] }) => hooks.get("before_agent_start")({ prompt, systemPromptOptions: options }, { sessionManager: { getBranch: () => branch } });
const pins = (options) => options.contextFiles.filter((f) => f.path.startsWith("memnest task card: ")).map((f) => f.path.slice(19));

start();
assert.equal(await run(), undefined, "no cards, nothing injected");
assert.match(await call({ action: "save", title: "", text: "x" }), /needs a title/);
assert.match(await call({ action: "save", title: "migration", text: "x".repeat(12001) }), /shorten/);

// Same title supersedes; the card is pinned once per run, after AGENTS.md.
await call({ action: "save", title: "migration", text: "v1 body" });
await call({ action: "save", title: "migration", text: "v2 body" });
assert.equal(adds[1].metadata.supersedes, "c1");
assert.equal(adds[1].project, "task-cards");
assert.ok(adds[1].text.startsWith("[memnest-task-card] migration\nv2 body\n\n## User's own words"), "body, then the verbatim log");
assert.match(adds[0].text, /\n- \d{4}-\d\d-\d\d \d\d:\d\d hi$/, "the session's earlier message is attached on first save");
const options = { contextFiles: [{ path: "/home/AGENTS.md", content: "rules" }] };
await run("hi", options);
await run("hi", options);
assert.deepEqual(options.contextFiles.map((f) => f.path), ["/home/AGENTS.md", "memnest task card: migration"]);
assert.match(options.contextFiles[1].content, /^The user's stated policies[\s\S]*\n\nv2 body\n\n## User's own words[\s\S]*hi$/);

// Up to three cards stay pinned; opening a fourth drops the oldest.
for (const t of ["blog", "tax", "trip"]) await call({ action: "save", title: t, text: `${t} body` });
const many = { contextFiles: [] };
await run("hi", many);
assert.deepEqual(pins(many), ["blog", "tax", "trip"]);

// A new session ranks cards by the first request and lists only active ones.
start();
const listed = await run("migration 오늘 진행 확인");
assert.match(listed.message.content, /most relevant to this request first[^\n]*\n- migration/);
assert.match(listed.message.content, /- trip/);
assert.equal(listed.message.display, false);
assert.equal(await run(), undefined, "listed only once per session");
assert.match(await call({ action: "list", text: "tax" }), /^- tax/);
assert.match(await call({ action: "get", title: "nope" }), /no task card titled "nope"/);
const opened = await call({ action: "get", title: "migration" });
assert.match(opened, /v2 body[\s\S]*\d\d:\d\d hi$/, "opening records the request that picked the task up");
assert.doesNotMatch(opened, /migration 오늘 진행 확인/, "earlier messages of the session may be other work and are not recorded");

// done moves a card out of the active collection; get still opens it; save reopens it.
assert.match(await call({ action: "done", title: "tax" }), /marked task card "tax" done/);
assert.deepEqual(updates.at(-1), { id: docs.find((d) => d.document.startsWith("[memnest-task-card] tax")).id, project: "task-cards-done" });
assert.doesNotMatch(await call({ action: "list" }), /tax/);
assert.match(await call({ action: "get", title: "tax" }), /marked done[\s\S]*tax body/);
assert.match(await call({ action: "done", title: "tax" }), /no active task card/);
await call({ action: "save", title: "tax", text: "tax reopened" });
assert.equal(updates.at(-1).project, "task-cards", "reopen moves the finished card back first");
assert.equal(docs.filter((d) => d.project === "task-cards" && d.document.includes(" tax\n")).length, 1, "one active tax card");
assert.match(await call({ action: "list" }), /- tax/);

// Compaction leaves the pin in place and asks for a save once.
branch = [...branch, { type: "compaction", id: "k1" }];
const afterCompact = { contextFiles: [] };
const reminder = await run("next", afterCompact);
assert.match(reminder.message.content, /just compacted[\s\S]*save it now/);
assert.ok(pins(afterCompact).length > 0);
assert.equal(await run("next", { contextFiles: [] }), undefined, "reminder only once");

// A resumed session reopens the cards it had open.
start([...entries]);
const resumed = { contextFiles: [] };
assert.equal(await run("hi", resumed), undefined);
assert.deepEqual(pins(resumed), entries.at(-1).data.titles);
assert.match(resumed.contextFiles.find((f) => f.path.endsWith("migration"))?.content ?? "", /v2 body/);

// User messages are appended verbatim while a card is open, and a save cannot rewrite them.
start();
await run("설정 변경은 의논 없이 하지 마");
await call({ action: "get", title: "migration" });
const cardDoc = () => docs.find((d) => d.project === "task-cards" && d.document.startsWith("[memnest-task-card] migration\n"));
assert.match(cardDoc().document, /의논 없이 하지 마$/);
await run("이미지는 다시 올려");
await new Promise((r) => setTimeout(r, 0));
assert.match(cardDoc().document, /의논 없이 하지 마\n- \S+ \S+ 이미지는 다시 올려$/, "written without a model call");
await call({ action: "save", title: "migration", text: "v3 body\n\n## User's own words (kept verbatim by memnest, oldest first)\n- 2026-01-01 00:00 forged" });
assert.doesNotMatch(cardDoc().document, /forged/, "the model cannot edit the log");
assert.match(cardDoc().document, /^\[memnest-task-card\] migration\nv3 body\n\n## User's own words[\s\S]*의논 없이 하지 마[\s\S]*다시 올려$/, "log carries over to the new version");
for (let i = 0; i < 20; i++) await run(`긴 메시지 ${i} ${"가".repeat(600)}`);
await new Promise((r) => setTimeout(r, 0));
assert.match(cardDoc().document, /older messages trimmed/);
assert.ok(cardDoc().document.length < 12000 + 8000 + 400, "log stays bounded");

// close stops recording; later unrelated messages stay out of the card.
assert.match(await call({ action: "close", title: "migration" }), /closed task card "migration"/);
await run("파이썬 CSV 스크립트 짜줘");
await new Promise((r) => setTimeout(r, 0));
assert.doesNotMatch(cardDoc().document, /CSV/);
assert.match(await call({ action: "close", title: "migration" }), /not open/);

// Two sessions recording into one card keep both messages.
const second = { tools: new Map(), hooks: new Map() };
(await import(`${bundle}?second`)).default({
	registerTool: (t) => second.tools.set(t.name, t),
	registerCommand() {},
	on: (name, fn) => { if (!second.hooks.has(name)) second.hooks.set(name, fn); },
	appendEntry() {},
});
const ctx2 = { sessionManager: { getBranch: () => [] } };
second.hooks.get("session_start")({}, ctx2);
await second.hooks.get("before_agent_start")({ prompt: "세션B 첫 말", systemPromptOptions: { contextFiles: [] } }, ctx2);
await second.tools.get("task_card").execute("1", { action: "get", title: "migration" }, null, null, {});
start();
await run("세션A 첫 말");
await call({ action: "get", title: "migration" });
await run("세션A 둘째 말");
await second.hooks.get("before_agent_start")({ prompt: "세션B 둘째 말", systemPromptOptions: { contextFiles: [] } }, ctx2);
await new Promise((r) => setTimeout(r, 10));
for (const said of ["세션A 첫 말", "세션A 둘째 말", "세션B 첫 말", "세션B 둘째 말"]) assert.match(cardDoc().document, new RegExp(said), `${said} kept`);

// A core that never answers must not block the first prompt.
const fetchImpl = globalThis.fetch;
globalThis.fetch = () => new Promise(() => {});
start();
const started = Date.now();
const keepAlive = setInterval(() => {}, 1000); // the extension's timer is unref'd
assert.equal(await run(), undefined);
clearInterval(keepAlive);
assert.ok(Date.now() - started < 3000, "listing gives up after its timeout");

// A full page of active cards asks the model to offer cleanup.
for (let i = 0; i < 50; i++) docs.push({ id: `bulk${i}`, project: "task-cards", document: `[memnest-task-card] bulk ${i}\nbody`, timestamp: "2026-10-01T00:00:00Z" });
globalThis.fetch = fetchImpl;
start();
assert.match((await run("hello")).message.content, /50 or more active cards/);

console.log("task cards: shared, supersede, 3 pins, ranked list, done/reopen, verbatim user log, close, two sessions, crowded nudge, compaction reminder, resume, stalled core skipped");
