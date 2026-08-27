#!/usr/bin/env node
// Deterministic hook tests for language-neutral Autocontext.
//
// The hook must search every substantive prompt verbatim, regardless of its
// language or wording, and let the semantic score decide whether to inject.

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
globalThis.fetch = async (_url, init) => {
	requests.push(JSON.parse(init.body));
	return new Response(
		JSON.stringify({
			project: "ws_workspace_1234",
			results: [
				{
					project: "playbook",
					document: "relevant durable memory",
					score,
					chunk_type: "Manual",
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
} catch (error) {
	console.error(error);
	fail++;
} finally {
	rmSync(tmp, { recursive: true, force: true });
}

console.log(`\nautocontext-triggers: ${ok} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
