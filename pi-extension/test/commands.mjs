#!/usr/bin/env node
// /memnest-cards and /memnest-search: list formatting, the project prefix, an unreachable or older
// core, no UI, and the card actions (open, close, mark done with a confirmation). Mocked pi and fetch.
process.env.TZ = "Asia/Seoul";
process.env.MEMNEST_URL = "http://127.0.0.1:3999";
delete process.env.MEMNEST_ALLOWED_MODEL_PROVIDERS;

import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const bundle = join(dirname(fileURLToPath(import.meta.url)), "..", "dist", "index.mjs");
const MARK = "[memnest-task-card]";
const LOG = "## User's own words (kept verbatim by memnest, oldest first)";

let door = true;
let down = false;
const requests = [];
const docs = [
	{ id: "c1", project: "task-cards", timestamp: "2026-11-11T14:31:00.123456789+00:00", document: `${MARK} deploy\nGoal: ship\n\n${LOG}\n- 2026-11-11 09:00 staging only` },
	{ id: "c2", project: "task-cards", timestamp: "2026-11-10T01:05:00+00:00", document: `${MARK} 블로그 이전\nbody two\x1b[31m red` },
	{ id: "c3", project: "task-cards-done", timestamp: "2026-11-01T00:00:00+00:00", document: `${MARK} old job\nfinished body` },
];
const recents = [
	{ id: "n1", project: "ws_demo_0123456789abcdef0123456789abcdef", score: 0, timestamp: "2026-11-11T17:11:32.892263876+00:00", created_at: "2026-11-10T01:00:00+00:00", chunk_type: "Manual", document: "Newest saved note" },
	{ id: "n2", project: "playbook", score: 0, timestamp: "2026-11-02T00:00:00+00:00", created_at: "2026-11-02T00:00:00+00:00", chunk_type: "Manual", document: "Older saved note" },
];
const rules = [
	{ id: "r1", project: "playbook", timestamp: "2026-09-01T00:00:00+00:00", created_at: "2026-09-01T00:00:00+00:00", chunk_type: "Manual", document: "Never deploy on Fridays" },
	{ id: "r2", project: "ws_demo_0123456789abcdef0123456789abcdef", timestamp: "2026-09-05T00:00:00+00:00", created_at: "2026-09-05T00:00:00+00:00", reviewed_at: "2026-10-20T03:00:00+00:00", chunk_type: "Manual", document: "Use tabs" },
	{ id: "r3", project: "playbook", timestamp: "2026-10-01T00:00:00+00:00", created_at: "2026-10-01T00:00:00+00:00", chunk_type: "Manual", document: "Answer in short sentences" },
];
let reviewDenied = false;
let searchReply = { results: [], project: "ws_x_0123456789abcdef0123456789abcdef" };
const mockFetch = async (url, init) => {
	const path = new URL(url).pathname;
	const body = init?.body ? JSON.parse(init.body) : undefined;
	requests.push({ path, body });
	if (down) throw new Error("connect ECONNREFUSED");
	const json = (value, status = 200) => new Response(JSON.stringify(value), { status });
	if (path === "/health") return json({ status: "ok", data_dir: "/tmp/x", capabilities: { task_card_log: door } });
	if (path === "/recent") return json({ project: "p", results: body.rules_only ? rules : recents });
	if (path === "/reviewed") return reviewDenied ? json({ status: "error", error: "project write access denied" }, 403) : json({ status: "ok", id: body.id });
	if (path === "/delete") return json({ status: "ok" });
	if (path === "/stats") return json({ collections: [{ name: "playbook" }, { name: "task-cards" }, { name: "ws_a_0123456789abcdef0123456789abcdef" }, { name: "my proj" }] });
	if (path === "/search" && ["task-cards", "task-cards-done"].includes(body.project))
		return json({ results: docs.filter((d) => d.project === body.project) });
	if (path === "/search") return json(searchReply);
	if (path.startsWith("/chunk/")) {
		const id = decodeURIComponent(path.slice(7));
		const d = docs.find((x) => x.id === id);
		return d ? json({ id, document: d.document, next_offset: null }) : json({ id, document: "full memory text\x1b[2J end\nsecond line", next_offset: null });
	}
	if (path === "/task-card") {
		if (!door) return json({ error: "not found" }, 404);
		docs.find((d) => d.document.startsWith(`${MARK} ${body.title}\n`)).project = "task-cards-done";
		return json({ status: "ok", text: "ok" });
	}
	if (path === "/update") {
		docs.find((d) => d.id === body.id).project = body.project;
		return json({ status: "ok" });
	}
	throw new Error(`unexpected ${path}`);
};

globalThis.fetch = mockFetch;
const realFetch = mockFetch;
const commands = new Map();
(await import(`${bundle}?commands`)).default({
	registerTool() {},
	registerCommand: (name, command) => commands.set(name, command),
	on() {},
	appendEntry() {},
});
for (const name of ["memnest", "memnest-cards", "memnest-search", "memnest-recent", "memnest-review"]) assert.ok(commands.has(name), `${name} registered`);

// A scripted UI: each select/confirm/input takes the next answer; a missing answer cancels.
function makeCtx({ hasUI = true, answers = [], cwd = "/work/demo" } = {}) {
	const shown = [];
	const asked = [];
	const next = (kind, ...args) => {
		asked.push({ kind, args });
		const answer = answers.shift();
		return typeof answer === "function" ? answer(...args) : answer;
	};
	return {
		shown, asked, cwd, hasUI,
		ui: {
			notify: (text, level = "info") => shown.push({ text, level }),
			select: async (title, options) => next("select", title, options),
			confirm: async (title, message) => next("confirm", title, message) === true,
			input: async (title) => next("input", title),
		},
	};
}
const pick = (prefix) => (_title, options) => {
	const hit = options.find((o) => o.includes(prefix));
	assert.ok(hit, `option containing "${prefix}" in ${JSON.stringify(options)}`);
	return hit;
};
const run = async (name, args, ctx) => commands.get(name).handler(args, ctx);
const calls = (path) => requests.filter((r) => r.path === path);

// --- /memnest-cards, UI: newest first, local time, finished cards, text with the user's words.
let ctx = makeCtx({ answers: [pick("deploy"), pick("Back"), undefined] });
await run("memnest-cards", "", ctx);
const [cardList, cardMenu] = ctx.asked.map((a) => a.args);
assert.match(cardList[0], /Active task cards: 2, newest first/);
assert.equal(cardList[1][0], "1. deploy (saved 2026-11-11 23:31)", "local time (UTC+9)");
assert.equal(cardList[1][1], "2. 블로그 이전 (saved 2026-11-10 10:05)");
assert.equal(cardList[1][2], "Finished cards...");
assert.match(ctx.shown[0].text, /^deploy\nsaved 2026-11-11 23:31\n\nGoal: ship[\s\S]*- 2026-11-11 09:00 staging only$/, "card text includes the user's own words");
assert.deepEqual(cardMenu[1], ["Open in this session (pin it, record your messages into it)", "Mark done", "Back"]);

// Stored text cannot carry terminal escapes; finished cards are reachable and read-only.
ctx = makeCtx({ answers: [pick("Finished"), pick("old job"), undefined, undefined] });
await run("memnest-cards", "", ctx);
assert.match(ctx.asked[1].args[0], /^Finished task cards: 1/);
assert.match(ctx.shown[0].text, /\(finished\)\n\nfinished body/);
ctx = makeCtx({ answers: [pick("블로그"), pick("Back"), undefined] });
await run("memnest-cards", "", ctx);
assert.ok(!ctx.shown[0].text.includes("\x1b"), "escape character removed");
assert.match(ctx.shown[0].text, /red/);

// Open, then it is marked and offers close; close again; mark done asks first and only then moves it.
ctx = makeCtx({ answers: [pick("deploy"), pick("Open in this session"), pick("[open] deploy"), pick("Close in this session"), undefined] });
await run("memnest-cards", "", ctx);
assert.match(ctx.shown.find((s) => /^Opened "deploy"/.test(s.text)).text, /pinned for this session/);
assert.match(ctx.asked[2].args[1][0], /^1\. \[open\] deploy \(saved/, "open mark in the list");
assert.match(ctx.shown.at(-1).text, /^Closed "deploy"/);
ctx = makeCtx({ answers: [pick("deploy"), pick("Mark done"), false, undefined] });
await run("memnest-cards", "", ctx);
assert.match(ctx.asked.find((a) => a.kind === "confirm").args[0], /Mark done\?/);
assert.equal(calls("/task-card").length, 0, "declined confirmation moves nothing");
assert.equal(docs[0].project, "task-cards");
ctx = makeCtx({ answers: [pick("deploy"), pick("Mark done"), true, undefined] });
await run("memnest-cards", "", ctx);
assert.equal(calls("/task-card").length, 1);
assert.deepEqual(calls("/task-card")[0].body, { action: "done", title: "deploy" });
assert.equal(docs[0].project, "task-cards-done");
assert.match(ctx.shown.at(-1).text, /^Marked "deploy" done/);
assert.match(ctx.asked.at(-1).args[0], /Active task cards: 1/, "list is read again");

// No UI: text only, both lists, nothing asked.
docs[0].project = docs[1].project = "task-cards";
const printed = [];
const realWrite = process.stdout.write.bind(process.stdout);
process.stdout.write = (chunk) => (printed.push(String(chunk)), true);
ctx = makeCtx({ hasUI: false });
await run("memnest-cards", "", ctx);
await run("memnest-search", "", ctx);
process.stdout.write = realWrite;
assert.equal(ctx.asked.length, 0);
const text = printed.join("");
assert.match(text, /Active task cards \(2\), newest first:\n {2}1\. deploy \(saved 2026-11-11 23:31\)\n {2}2\. 블로그 이전/);
assert.match(text, /Finished task cards \(1\), newest first:\n {2}1\. old job/);
assert.match(text, /Usage: \/memnest-search \[project:<name>\|project:all\] <question>/);

// Unreachable core: one line, for both commands.
down = true;
for (const [name, args] of [["memnest-cards", ""], ["memnest-search", "anything"], ["memnest-recent", ""], ["memnest-review", ""]]) {
	ctx = makeCtx({ answers: [] });
	await run(name, args, ctx);
	assert.deepEqual(ctx.shown, [{ text: "Memnest is not reachable at http://127.0.0.1:3999", level: "error" }]);
	assert.equal(ctx.asked.length, 0);
}
down = false;

// --- /memnest-search: scope words.
searchReply = { results: [], project: "ws_x" };
const searchBody = async (args, c = makeCtx()) => (await run("memnest-search", args, c), calls("/search").at(-1).body);
let body = await searchBody("where is the port");
assert.deepEqual(body, { query: "where is the port", project: "", cwd: "/work/demo", n_results: 10, adapter: "pi" }, "folder scope, the request memory_search sends");
body = await searchBody("project:playbook how to reply");
assert.deepEqual([body.project, body.cwd, body.query], ["playbook", undefined, "how to reply"]);
body = await searchBody("PROJECT:all  port 5544 ");
assert.deepEqual([body.project, body.cwd, body.query], ["all", undefined, "port 5544"]);
body = await searchBody("see project:x later");
assert.equal(body.project, "", "only a leading word picks the project");
ctx = makeCtx({ cwd: "" });
await run("memnest-search", "port", ctx);
assert.match(ctx.shown[0].text, /current folder is unknown/);
// No argument: ask for the question; an empty answer stops.
ctx = makeCtx({ answers: ["typed question"] });
await run("memnest-search", "", ctx);
assert.equal(ctx.asked[0].kind, "input");
assert.equal(calls("/search").at(-1).body.query, "typed question");
const before = calls("/search").length;
ctx = makeCtx({ answers: ["  "] });
await run("memnest-search", "", ctx);
assert.equal(calls("/search").length, before);
// "project:x" alone asks for the question too.
ctx = makeCtx({ answers: ["q2"] });
await run("memnest-search", "project:playbook", ctx);
assert.deepEqual([calls("/search").at(-1).body.project, calls("/search").at(-1).body.query], ["playbook", "q2"]);

// --- results: project, date, type, score, one-line excerpt, scope hint; choosing shows the full text.
const long = "가".repeat(300);
searchReply = {
	project: "ws_demo_0123456789abcdef0123456789abcdef",
	scope_hint: "Stronger matches in other projects: playbook.\nSearch again with project:all",
	results: [
		{ id: "m1", project: "ws_demo_0123456789abcdef0123456789abcdef", score: 0.2491, timestamp: "2026-11-11T17:11:32.892263876+00:00", chunk_type: "Manual", document: `first line\n\twith\x1b[31m break ${long}` },
		{ id: "m2", project: "playbook", score: 0.1, timestamp: "2026-11-02T00:00:00+00:00", chunk_type: "AutoLog", document: "short" },
	],
};
ctx = makeCtx({ answers: [pick("1. demo"), undefined] });
await run("memnest-search", "port", ctx);
const [resultTitle, rows] = ctx.asked[0].args;
assert.match(resultTitle, /^2 results for "port" in this folder\nStronger matches in other projects: playbook\. Search again with project:all$/);
assert.match(rows[0], /^1\. demo · 2026-11-12 · manual · 0\.25 · first line with \[31m break 가/, "project short name, local date, type, score, one-line excerpt");
assert.match(rows[1], /^2\. playbook · 2026-11-02 · autolog · 0\.10 · short$/);
assert.ok(!/[\n\t\x1b]/.test(rows[0]));
const cellWidth = Array.from(rows[0]).reduce((n, ch) => n + (ch.codePointAt(0) > 0x1100 ? 2 : 1), 0);
assert.ok(cellWidth <= 94, `row fits 100 columns (${cellWidth})`);
assert.ok(rows[0].endsWith("…"));
assert.equal(ctx.asked.length, 2, "list is shown again after a result");
assert.deepEqual(calls("/chunk/m1").length, 1);
assert.match(ctx.shown[0].text, /^demo · 2026-11-12 · manual · id m1\n\nfull memory text {0,2}\[2J end\nsecond line$|^demo · 2026-11-12 · manual · id m1\n\nfull memory text.*end\nsecond line$/s);
assert.ok(!ctx.shown[0].text.includes("\x1b"));

// A search without UI prints the rows and the hint; no results prints one line plus the hint.
process.stdout.write = (chunk) => (printed.push(String(chunk)), true);
printed.length = 0;
await run("memnest-search", "project:all port", makeCtx({ hasUI: false }));
searchReply = { results: [], scope_hint: "Try project:all" };
await run("memnest-search", "nothing", makeCtx({ hasUI: false }));
process.stdout.write = realWrite;
assert.match(printed.join(""), /2 results for "port" in project all:\n {2}1\. demo · [\s\S]*\n {2}2\. playbook[\s\S]*Stronger matches/);
assert.match(printed.join(""), /No results for "nothing" in this folder\.\nTry project:all/);

// --- /memnest-recent: the newest saved memories, same rows and full-text view as search.
ctx = makeCtx({ answers: [pick("Newest saved"), undefined] });
await run("memnest-recent", "", ctx);
assert.deepEqual(calls("/recent").at(-1).body, { project: "", cwd: "/work/demo", rules_only: false, oldest_first: false, limit: 20 });
assert.equal(ctx.asked[0].args[0], "2 newest saved memories in this folder");
assert.deepEqual(ctx.asked[0].args[1], ["1. demo · 2026-11-12 · manual · Newest saved note", "2. playbook · 2026-11-02 · manual · Older saved note"]);
assert.match(ctx.shown[0].text, /^demo · 2026-11-12 · manual · id n1\n\nfull memory text/);
assert.equal(ctx.asked.length, 2, "list is shown again");
await run("memnest-recent", "project:all", makeCtx({ answers: [] }));
assert.deepEqual([calls("/recent").at(-1).body.project, calls("/recent").at(-1).body.cwd], ["all", undefined]);
// No UI: printed, nothing asked.
printed.length = 0;
process.stdout.write = (chunk) => (printed.push(String(chunk)), true);
ctx = makeCtx({ hasUI: false });
await run("memnest-recent", "", ctx);
process.stdout.write = realWrite;
assert.equal(ctx.asked.length, 0);
assert.match(printed.join(""), /2 newest saved memories in this folder:\n {2}1\. demo · 2026-11-12 · manual · Newest saved note\n {2}2\. playbook/);

// --- /memnest-review: rules oldest first; keep, delete, skip, stop; delete only on its own choice.
const KEEP = (_t, options) => options.find((o) => o.startsWith("Keep"));
const DELETE = (_t, options) => options.find((o) => o.startsWith("Delete"));
const SKIP = (_t, options) => options.find((o) => o.startsWith("Skip"));
const STOP = (_t, options) => options.find((o) => o === "Stop");
const reviewed = () => calls("/reviewed").map((r) => r.body.id);
const deleted = () => calls("/delete").flatMap((r) => r.body.ids);
const start = { reviewed: reviewed().length, deleted: deleted().length };
ctx = makeCtx({ answers: [KEEP, DELETE, SKIP] });
await run("memnest-review", "", ctx);
assert.deepEqual(calls("/recent").at(-1).body, { project: "", cwd: "/work/demo", rules_only: true, oldest_first: true, limit: 50 });
assert.deepEqual(reviewed().slice(start.reviewed), ["r1"]);
assert.deepEqual(deleted().slice(start.deleted), ["r2"]);
assert.match(ctx.shown[0].text, /^1 of 3: playbook · saved 2026-09-01 · never reviewed · id r1\n\nfull memory text/);
assert.match(ctx.shown[1].text, /^2 of 3: demo · saved 2026-09-05 · reviewed 2026-10-20 · id r2/);
assert.match(ctx.asked[0].args[0], /^Rule 1 of 3: does it still hold\?/);
assert.deepEqual(ctx.asked[0].args[1].map((o) => o.split(" (")[0]), ["Keep", "Delete", "Skip", "Stop"]);
assert.match(ctx.asked[0].args[1][1], /restorable for 30 days/, "the prompt says the trash is restorable");
assert.equal(ctx.shown.at(-1).text, "Review done: 1 kept, 1 deleted, 1 skipped. Deleted memories stay in the trash for 30 days.");
// Stop and Escape leave the rest untouched; Escape never deletes.
const mark = { r: reviewed().length, d: deleted().length };
ctx = makeCtx({ answers: [KEEP, STOP] });
await run("memnest-review", "project:playbook", ctx);
assert.equal(calls("/recent").at(-1).body.project, "playbook");
assert.equal(ctx.shown.at(-1).text, "Review done: 1 kept, 0 deleted, 0 skipped, 2 not looked at.");
ctx = makeCtx({ answers: [undefined] });
await run("memnest-review", "", ctx);
assert.equal(ctx.shown.at(-1).text, "Review done: 0 kept, 0 deleted, 0 skipped, 3 not looked at.");
assert.equal(reviewed().length - mark.r, 1);
assert.equal(deleted().length - mark.d, 0, "nothing deleted without choosing Delete");
// A refused write is shown and counted as skipped, not as kept.
reviewDenied = true;
ctx = makeCtx({ answers: [KEEP, STOP] });
await run("memnest-review", "", ctx);
assert.deepEqual(ctx.shown.find((s) => s.level === "error"), { text: 'memnest error 403: {"status":"error","error":"project write access denied"}', level: "error" });
assert.match(ctx.shown.at(-1).text, /^Review done: 0 kept, 0 deleted, 1 skipped, 2 not looked at\./);
reviewDenied = false;
// No UI: only the list; nothing is written.
const writes = reviewed().length + deleted().length;
printed.length = 0;
process.stdout.write = (chunk) => (printed.push(String(chunk)), true);
ctx = makeCtx({ hasUI: false });
await run("memnest-review", "", ctx);
process.stdout.write = realWrite;
assert.equal(ctx.asked.length, 0);
assert.equal(reviewed().length + deleted().length, writes);
assert.match(printed.join(""), /3 rules and preferences in this folder, longest unconfirmed first:\n {2}1\. playbook · saved 2026-09-01 · never reviewed · id r1 · Never deploy on Fridays\n {2}2\. demo · saved 2026-09-05 · reviewed 2026-10-20/);

// An error from the core (too old for the route) is one line.
globalThis.fetch = async () => new Response("not found", { status: 404 });
ctx = makeCtx();
await run("memnest-search", "port", ctx);
assert.deepEqual(ctx.shown, [{ text: "This Memnest core is too old for that command; update the core", level: "error" }]);
for (const name of ["memnest-recent", "memnest-review"]) {
	ctx = makeCtx({ answers: [] });
	await run(name, "", ctx);
	assert.deepEqual(ctx.shown, [{ text: "This Memnest core is too old for that command; update the core", level: "error" }], name);
	assert.equal(ctx.asked.length, 0);
}

// An older core without /task-card (its own extension instance, since /health is asked once): done goes through /update, as the tool does.
{
	door = false;
	docs[1].project = "task-cards";
	globalThis.fetch = realFetch;
	const old = new Map();
	(await import(`${bundle}?old-core`)).default({ registerTool() {}, registerCommand: (n, c) => old.set(n, c), on() {}, appendEntry() {} });
	const c = makeCtx({ answers: [pick("블로그"), pick("Mark done"), true, undefined] });
	await old.get("memnest-cards").handler("", c);
	assert.deepEqual(calls("/update").at(-1).body, { id: "c2", project: "task-cards-done" });
	assert.equal(calls("/task-card").length, 1, "only the earlier call on the new core");
	assert.match(c.shown.at(-1).text, /^Marked "블로그 이전" done/);
	door = true;
}

// --- completion: project names after "project:", reserved collections hidden, nothing for other words.
globalThis.fetch = async () => new Response(JSON.stringify({ collections: [{ name: "playbook" }, { name: "task-cards" }, { name: "task-cards-done" }, { name: "ws_a_0123456789abcdef0123456789abcdef" }, { name: "my proj" }] }), { status: 200 });
const complete = commands.get("memnest-search").getArgumentCompletions;
assert.deepEqual((await complete("project:")).map((i) => i.value), ["project:all", "project:playbook", "project:ws_a_0123456789abcdef0123456789abcdef"]);
assert.deepEqual((await complete("project:p")).map((i) => i.value), ["project:playbook"]);
assert.deepEqual((await complete("pro")).map((i) => i.value).slice(0, 1), ["project:all"]);
assert.equal(await complete("port 5544"), null);
assert.equal(await complete("where"), null);
assert.equal(await complete(""), null);

for (const name of ["memnest-recent", "memnest-review"]) assert.deepEqual((await commands.get(name).getArgumentCompletions("project:p")).map((i) => i.value), ["project:playbook"]);

console.log("commands: cards, search, recent and review (keep/delete/skip/stop), no UI, unreachable and older core, completion");
