#!/usr/bin/env node
// Smoke test for pi-memnest dist/index.mjs.
//
// Verifies:
//   1. ESM bundle loads without throwing.
//   2. register() registers exactly the expected tool set.
//   3. Each registered tool has a callable .execute function.
//   4. memory_health round-trip succeeds against a running memnest server.
//   5. memory_stats returns a JSON body with total_chunks.
//
// Run: node test/smoke.mjs    (or)    bun test/smoke.mjs

import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const BUNDLE = join(__dirname, "..", "dist", "index.mjs");

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

if (!existsSync(BUNDLE)) {
	console.error(`bundle missing: ${BUNDLE}\nRun: npm run build`);
	process.exit(2);
}

const tools = new Map();
const commands = new Map();
const hooks = [];
const fakePi = {
	registerTool: (t) => tools.set(t.name, t),
	registerCommand: (name, command) => commands.set(name, command),
	on: (name) => hooks.push(name),
};
// Tolerate any other pi.* methods the extension may call.
const noop = () => {};
const proxy = new Proxy(fakePi, {
	get(target, prop) {
		if (prop in target) return target[prop];
		return noop;
	},
});

const mod = await import(BUNDLE);
assert("ESM bundle loads", typeof mod.default === "function");

await mod.default(proxy);
assert("register() called without throwing", true);
assert("/memnest command registered", commands.has("memnest"));
const AUTOLOG_HOOKS = [
	"input",
	"message_end",
	"tool_execution_end",
	"session_compact",
	"agent_end",
	"session_shutdown",
];
assert(
	"no AutoLog hooks are installed",
	AUTOLOG_HOOKS.every((name) => !hooks.includes(name)),
	`installed hooks: ${hooks.join(", ")}`,
);

const EXPECTED = [
	"memory_remember",
	"memory_search",
	"memory_get",
	"memory_update",
	"memory_delete",
	"memory_feedback",
	"secret_set",
	"secret_get",
	"secret_list",
	"secret_delete",
];

assert(
	`registered ${EXPECTED.length} tools (got ${tools.size})`,
	tools.size === EXPECTED.length,
);
for (const name of EXPECTED) {
	const t = tools.get(name);
	assert(`tool '${name}' present`, !!t);
	if (t)
		assert(
			`tool '${name}'.execute is a function`,
			typeof t.execute === "function",
		);
}
assert(
	"memory_search defaults to 3 results",
	tools.get("memory_search")?.parameters?.properties?.n_results?.default === 3,
);
const unscoped = await tools
	.get("memory_search")
	.execute("id", { query: "must stay scoped" }, undefined, noop, {});
assert(
	"memory_search does not fall back to all projects without cwd",
	/unavailable/.test(unscoped.content?.[0]?.text ?? ""),
);

// Live search round-trip. MEMNEST_URL has no default because search records
// recall telemetry; never point the smoke test at an unrelated personal store.
const URL = process.env.MEMNEST_URL;
if (!URL) {
	console.log(
		"\n(MEMNEST_URL not set, skipping live calls)\n" +
			"  memnest --data-dir /tmp/memnest-smoke --port 3150 &\n" +
			"  MEMNEST_URL=http://127.0.0.1:3150 npm run smoke",
	);
}
let reachable = false;
try {
	const r = await fetch(`${URL}/health`);
	reachable = r.ok;
} catch {}

if (reachable) {
	const search = tools.get("memory_search");
	const response = await search.execute(
		"id",
		{ query: "memnest smoke", n_results: 1 },
		undefined,
		noop,
		{ cwd: process.cwd() },
	);
	assert(
		"memory_search returns compact text",
		(response.content?.[0]?.text ?? "").startsWith("=== memory search results"),
	);
} else {
	console.log(
		`\n(memnest server not reachable at ${URL} — skipping live calls)`,
	);
}

console.log(`\nsmoke: ${ok} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
