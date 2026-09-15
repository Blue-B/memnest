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

const requests = [];
let score = 0.5;
let overrides = {};
globalThis.fetch = async (_url, init) => {
	requests.push(JSON.parse(init.body));
	return new Response(
		JSON.stringify({
			project: "ws_workspace_1234",
			results: [
				{
					id: "memory-123",
					project: "playbook",
					document: "relevant durable memory",
					score,
					timestamp: "2026-01-01T00:00:00Z",
					chunk_type: "Manual",
					...overrides,
				},
			],
		}),
		{ status: 200, headers: { "content-type": "application/json" } },
	);
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
