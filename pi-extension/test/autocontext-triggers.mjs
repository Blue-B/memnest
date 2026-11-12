#!/usr/bin/env node
// Deterministic tests: opt-in semantic recall and no automatic recall by default.
// No live service, external model, or language-specific intent classifier is used.

import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

process.env.MEMNEST_AUTOCONTEXT_MODE = "balanced";
process.env.MEMNEST_AUTOCONTEXT_MIN_SCORE = "0.25";
delete process.env.MEMNEST_AUTOCONTEXT_DISABLE;

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const ESBUILD = join(ROOT, "node_modules", ".bin", "esbuild");

let ok = 0,
	fail = 0;
function assert(name, cond, msg = "") {
	if (cond) {
		ok++;
		console.log(`  PASS  ${name}`);
	} else {
		fail++;
		console.log(`  FAIL  ${name}  -- ${msg}`);
	}
}

// `requests` holds /search bodies and `contextRequests` /context bodies.
const requests = [];
const contextRequests = [];
let score = 0.5;
let overrides = {};
// What answers /context: "old" replies without the on-topic gate's marker, as
// a core from before the gate does; "gated" replies with it; "none" has no
// such endpoint; "down" does not answer.
let core = "old";
let gatedMemories = [];
const memory = (extra = {}) => ({
	id: "memory-123",
	project: "playbook",
	document: "relevant durable memory",
	score,
	timestamp: "2026-01-01T00:00:00Z",
	chunk_type: "Manual",
	...overrides,
	...extra,
});
const json = (body) =>
	new Response(JSON.stringify(body), {
		status: 200,
		headers: { "content-type": "application/json" },
	});
globalThis.fetch = async (url, init) => {
	const body = JSON.parse(init.body);
	if (String(url).endsWith("/context")) {
		contextRequests.push(body);
		if (core === "down") throw new Error("connection refused");
		if (core === "none") return new Response("not found", { status: 404 });
		return json({
			query: body.query,
			project: "ws_workspace_1234",
			memories: core === "gated" ? gatedMemories : [memory()],
			prompt: "",
			...(core === "gated" ? { on_topic_only: true } : {}),
		});
	}
	requests.push(body);
	return json({ project: "ws_workspace_1234", results: [memory()] });
};

const tmp = mkdtempSync(join(tmpdir(), "memnest-autocontext-"));
const outfile = join(tmp, "autocontext.mjs");
try {
	execFileSync(
		ESBUILD,
		[
			join(ROOT, "src", "autocontext.ts"),
			"--bundle",
			"--format=esm",
			"--platform=node",
			"--target=node20",
			`--outfile=${outfile}`,
		],
		{ stdio: ["ignore", "ignore", "inherit"] },
	);
	const { installAutocontext, isSubstantive } = await import(
		pathToFileURL(outfile).href
	);
	const hooks = new Map();
	installAutocontext({
		registerTool() {},
		on(name, fn) {
			hooks.set(name, fn);
		},
	});
	const before = hooks.get("before_agent_start");
	const start = hooks.get("session_start");
	assert("hook: before_agent_start registered", typeof before === "function");
	assert("hook: session_start registered", typeof start === "function");

	async function run(prompt, cwd) {
		start({}, { cwd });
		const count = requests.length;
		const result = await before({ prompt });
		return { result, request: requests[count], searched: requests.length - count };
	}

	for (const [name, prompt] of [
		["Korean", "개발 서버를 실행하는 방법과 포트를 알려줘"],
		["German", "Wie starte ich den Entwicklungsserver und welchen Port nutzt er?"],
		["Japanese", "開発サーバーの起動方法とポートを教えてください"],
		["Spanish", "¿Cómo inicio el servidor de desarrollo y qué puerto usa?"],
	]) {
		const { result, request, searched } = await run(prompt, "/tmp/workspace");
		assert(`${name}: substantive prompt is searched`, searched === 1);
		assert(`${name}: query is not rewritten`, request?.query === prompt);
		assert(`${name}: high-score memory is injected`, !!result?.message?.content);
	}

	const neutral = await run(
		"rename the local variable to userCount please",
		"/tmp/workspace",
	);
	assert("neutral wording: still searched", neutral.searched === 1);

	score = 0.24;
	const weak = await run(
		"check whether a previous implementation detail matters here",
		"/tmp/workspace",
	);
	assert("score gate: weak result is still searched", weak.searched === 1);
	assert("score gate: below 0.25 is not injected", weak.result === undefined);

	score = 0.25;
	const threshold = await run(
		"check whether a stored implementation detail matters here",
		"/tmp/workspace",
	);
	assert("score gate: threshold result is injected", !!threshold.result?.message?.content);
	assert("card: shows an ID for memory_get", threshold.result?.message?.content.includes('id="memory-123"'));
	assert("card: shows creation time, not verified truth", threshold.result?.message?.content.includes("created=2026-01-01T00:00:00Z"));

	for (const [name, invalid] of [
		["missing ID", { id: undefined }],
		["missing score", { score: undefined }],
		["string score", { score: "0.9" }],
		["nonfinite score", { score: Infinity }],
		["missing project", { project: undefined }],
		["superseded decision", { project: "_superseded" }],
		["other workspace", { project: "other" }],
		["unfinished transcript", { chunk_type: "AutoLog" }],
		["empty text", { document: "  " }],
	]) {
		overrides = invalid;
		const rejected = await run("recall the current deployment port decision", "/tmp/workspace");
		assert(`fail closed: ${name}`, rejected.result === undefined);
	}
	overrides = {};

	const noCwd = await run("find any durable context for this substantive request");
	assert("scope: unknown cwd does not search globally", noCwd.searched === 0);
	assert("scope: unknown cwd injects nothing", noCwd.result === undefined);

	start({}, { cwd: "/tmp/workspace" });
	const duplicatePrompt = "look up the stored deployment procedure before answering";
	const count = requests.length;
	await before({ prompt: duplicatePrompt });
	const duplicate = await before({ prompt: duplicatePrompt });
	assert("duplicate: exact repeated prompt searches once", requests.length - count === 1);
	assert("duplicate: second invocation injects nothing", duplicate === undefined);

	assert("older core: /context is asked once, then only /search", contextRequests.length === 1);

	// A core with the on-topic gate decides what is related; the score is not consulted.
	async function fresh(tag) {
		const module = await import(`${pathToFileURL(outfile).href}?${tag}`);
		const freshHooks = new Map();
		module.installAutocontext({ registerTool() {}, on: (name, fn) => freshHooks.set(name, fn) });
		return async (prompt) => {
			freshHooks.get("session_start")({}, { cwd: "/tmp/workspace" });
			const [searches, asks] = [requests.length, contextRequests.length];
			const result = await freshHooks.get("before_agent_start")({ prompt });
			return {
				result,
				searched: requests.length - searches,
				asked: contextRequests.length - asks,
				request: contextRequests[asks],
			};
		};
	}
	core = "gated";
	const gated = await fresh("gated");
	gatedMemories = [memory({ score: -0.12 })];
	const onTopic = await gated("배포 포트가 어떻게 되지? staging 기준으로");
	assert("gate: asks /context once and never /search", onTopic.asked === 1 && onTopic.searched === 0);
	assert(
		"gate: sends the prompt and the workspace, no project",
		JSON.stringify(onTopic.request) ===
			JSON.stringify({ query: "배포 포트가 어떻게 되지? staging 기준으로", project: "", cwd: "/tmp/workspace", n_results: 20 }),
		JSON.stringify(onTopic.request),
	);
	assert("gate: a kept memory is injected whatever its score", onTopic.result?.message?.content.includes('id="memory-123"'));
	assert("gate: the card says which gate chose it", onTopic.result?.message?.content.includes("(on-topic-gate)"));
	gatedMemories = [];
	const vague = await gated("그럼 이제 어떻게 해? 다시 해줘 일단");
	assert("gate: a prompt the core keeps nothing for injects nothing", vague.result === undefined && vague.searched === 0);
	gatedMemories = [memory({ project: "other" }), memory({ chunk_type: "AutoLog" }), memory({ project: "root" })];
	const foreign = await gated("recall the current deployment port decision");
	assert("gate: other projects, transcripts and excluded collections stay out", foreign.result === undefined);
	core = "down";
	const down = await gated("what did we decide about the deploy port again");
	assert("gate: an unreachable core injects nothing and is not searched", down.result === undefined && down.searched === 0);
	core = "gated";
	gatedMemories = [memory({ score: 0 })];
	const back = await gated("what did we decide about the staging deploy port");
	assert("gate: one failure does not switch to the score path", back.asked === 1 && back.searched === 0 && !!back.result);

	core = "none";
	score = 0.5;
	const without = await (await fresh("no-context"))("what did we decide about the deploy port");
	assert("core without /context: falls back to /search and its score", without.searched === 1 && !!without.result?.message?.content.includes("(semantic-score-gate)"));
	core = "old";

	assert("substantive: slash command is skipped", !isSubstantive("/memnest status"));
	assert("substantive: trivial reply is skipped", !isSubstantive("okay"));
	assert(
		"substantive: real prompt passes",
		isSubstantive("please refactor the retry loop in the http client"),
	);

	// Even a high-scoring but irrelevant memory must not be searched or injected
	// without explicit opt-in. This tests the policy, not embedding accuracy.
	score = 0.99;
	overrides = { document: "an unrelated relay repair and obsolete port" };
	for (const mode of [undefined, "off", "none", "typo"]) {
		if (mode === undefined) delete process.env.MEMNEST_AUTOCONTEXT_MODE;
		else process.env.MEMNEST_AUTOCONTEXT_MODE = mode;
		const fresh = await import(`${pathToFileURL(outfile).href}?mode=${mode}`);
		const quietHooks = new Map();
		assert(`mode ${mode}: disabled`, fresh.installAutocontext({ on: (name, fn) => quietHooks.set(name, fn) }) === false);
		const count = requests.length;
		for (const prompt of [
			"다른 작업이나 파일 변경 없이 PASEO_PI_OK라고만 답하세요.",
			"rename the local variable to userCount please",
			"Translate this sentence into Korean without using earlier context.",
			"현재 배포 포트가 8420으로 바뀌었습니다. 이전 값을 사용하지 마세요.",
		]) {
			const response = await quietHooks.get("before_agent_start")?.({ prompt });
			assert(`mode ${mode}: no unsolicited context`, response === undefined);
		}
		assert(`mode ${mode}: no retrieval requests`, requests.length === count);
	}
	process.env.MEMNEST_AUTOCONTEXT_MODE = "balanced";
	process.env.MEMNEST_AUTOCONTEXT_DISABLE = "1";
	const disabled = await import(`${pathToFileURL(outfile).href}?disabled`);
	assert("disable flag overrides opt-in", disabled.installAutocontext({ on() { throw new Error("must not register"); } }) === false);
} catch (error) {
	console.error(error);
	fail++;
} finally {
	rmSync(tmp, { recursive: true, force: true });
}

console.log(`\nautocontext-triggers: ${ok} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
