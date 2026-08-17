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
const fakePi = {
	registerTool: (t) => tools.set(t.name, t),
	registerCommand: (name, command) => commands.set(name, command),
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

const EXPECTED = [
	"memory_remember",
	"memory_update",
	"memory_search",
	"memory_feedback",
	"memory_get",
	"memory_context",
	"memory_stats",
	"memory_sessions",
	"memory_facts_list",
	"note_set",
	"note_get",
	"notes_list",
	"note_delete",
	"secret_set",
	"secret_get",
	"secret_list",
	"secret_delete",
	"collections_list",
	"memory_health",
	"memnest_autocontext_status",
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
assert(
	"memory_context defaults to 2000 chars",
	tools.get("memory_context")?.parameters?.properties?.max_chars?.default ===
		2000,
);

const autocontext = tools.get("memnest_autocontext_status");
const ac = await autocontext.execute("id", {}, undefined, noop, {
	cwd: process.cwd(),
});
const acText = ac.content?.[0]?.text ?? "";
assert(
	"memnest_autocontext_status reports mode",
	/mode\s+:/.test(acText),
	acText.slice(0, 200),
);

// Live server round-trip. MEMNEST_URL has no default because these calls store
// memories, and a default of 127.0.0.1:3111 would write test collections into
// whatever store the developer actually uses.
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
	const health = tools.get("memory_health");
	const r1 = await health.execute("id", {}, undefined, noop, {
		cwd: process.cwd(),
	});
	const t1 = r1.content?.[0]?.text ?? "";
	assert(
		"memory_health returns JSON 'ok'",
		t1.includes('"ok"'),
		t1.slice(0, 200),
	);

	const stats = tools.get("memory_stats");
	const r2 = await stats.execute("id", {}, undefined, noop, {
		cwd: process.cwd(),
	});
	const t2 = r2.content?.[0]?.text ?? "";
	assert(
		"memory_stats returns total_chunks",
		/total_chunks/.test(t2),
		t2.slice(0, 200),
	);

	const cols = tools.get("collections_list");
	const r3 = await cols.execute("id", {}, undefined, noop, {
		cwd: process.cwd(),
	});
	const t3 = r3.content?.[0]?.text ?? "";
	assert(
		"collections_list returns an array",
		t3.trim().startsWith("["),
		t3.slice(0, 200),
	);

	const search = tools.get("memory_search");
	const r4 = await search.execute(
		"id",
		{ query: "memnest smoke", n_results: 1 },
		undefined,
		noop,
		{ cwd: process.cwd() },
	);
	const t4 = r4.content?.[0]?.text ?? "";
	assert(
		"memory_search returns compact text",
		t4.startsWith("=== memory search results"),
		t4.slice(0, 200),
	);
	assert(
		"memory_search omits raw response metadata",
		!/"elapsed_ms"|"timestamp"/.test(t4),
		t4.slice(0, 200),
	);
	const recallMatch = t4.match(/recall_id=([\w-]+)/);
	if (recallMatch) {
		const feedback = tools.get("memory_feedback");
		const rf = await feedback.execute(
			"id",
			{ recall_id: recallMatch[1], outcome: "ignored" },
			undefined,
			noop,
			{ cwd: process.cwd() },
		);
		const tf = rf.content?.[0]?.text ?? "";
		if (/memnest error 404/.test(tf)) {
			console.log("  SKIP  memory_feedback live call (old server)");
		} else {
			assert("memory_feedback accepts recall_id", /"status":"ok"/.test(tf), tf);
		}
	}

	// Full-text escape hatch: take an id from the search output and fetch it.
	const idMatch = t4.match(/(?:^|\n)\[\d+\].*\bid=([\w-]+)/);
	if (idMatch) {
		const getTool = tools.get("memory_get");
		const rg1 = await getTool.execute(
			"id",
			{ id: idMatch[1] },
			undefined,
			noop,
			{
				cwd: process.cwd(),
			},
		);
		const tg = rg1.content?.[0]?.text ?? "";
		if (/memnest error 404/.test(tg)) {
			console.log(
				"  SKIP  memory_get live call (server does not expose /chunk yet)",
			);
		} else {
			assert(
				"memory_get returns the full document",
				tg.startsWith(`id=${idMatch[1]}`) && tg.includes("\n"),
				tg.slice(0, 200),
			);
		}
	} else {
		console.log("  SKIP  memory_get (no id in search output)");
	}

	const ctx = tools.get("memory_context");
	const r5 = await ctx.execute(
		"id",
		{ query: "memnest smoke", n_results: 1 },
		undefined,
		noop,
		{ cwd: process.cwd() },
	);
	const t5 = r5.content?.[0]?.text ?? "";
	if (/memnest error 404/.test(t5)) {
		console.log(
			"  SKIP  memory_context live call (server does not expose /context yet)",
		);
	} else {
		assert(
			"memory_context returns only the prompt",
			/memnest_context/.test(t5) && !t5.trim().startsWith("{"),
			t5.slice(0, 200),
		);
		assert(
			"memory_context respects compact default",
			t5.length <= 2000,
			`got ${t5.length} chars`,
		);
	}
} else {
	console.log(
		`\n(memnest server not reachable at ${URL} — skipping live calls)`,
	);
}

console.log(`\nsmoke: ${ok} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
