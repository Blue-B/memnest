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

let ok = 0, fail = 0;
function assert(name, cond, msg = "") {
  if (cond) { ok++; console.log(`  PASS  ${name}`); }
  else { fail++; console.log(`  FAIL  ${name}  -- ${msg}`); }
}

if (!existsSync(BUNDLE)) {
  console.error(`bundle missing: ${BUNDLE}\nRun: npm run build`);
  process.exit(2);
}

const tools = new Map();
const fakePi = { registerTool: (t) => tools.set(t.name, t) };
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

const EXPECTED = [
  "memory_remember",
  "memory_update",
  "memory_search",
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

assert(`registered ${EXPECTED.length} tools (got ${tools.size})`, tools.size === EXPECTED.length);
for (const name of EXPECTED) {
  const t = tools.get(name);
  assert(`tool '${name}' present`, !!t);
  if (t) assert(`tool '${name}'.execute is a function`, typeof t.execute === "function");
}

const autocontext = tools.get("memnest_autocontext_status");
const ac = await autocontext.execute("id", {}, undefined, noop, { cwd: process.cwd() });
const acText = ac.content?.[0]?.text ?? "";
assert("memnest_autocontext_status reports mode", /mode\s+:/.test(acText), acText.slice(0, 200));

// Live server round-trip (skipped gracefully if 3111 is down).
const URL = process.env.MEMNEST_URL ?? "http://127.0.0.1:3111";
let reachable = false;
try {
  const r = await fetch(`${URL}/health`);
  reachable = r.ok;
} catch {}

if (!reachable) {
  console.log(`\n(memnest server not reachable at ${URL} — skipping live calls)`);
} else {
  const health = tools.get("memory_health");
  const r1 = await health.execute("id", {}, undefined, noop, { cwd: process.cwd() });
  const t1 = r1.content?.[0]?.text ?? "";
  assert("memory_health returns JSON 'ok'", t1.includes("\"ok\""), t1.slice(0, 200));

  const stats = tools.get("memory_stats");
  const r2 = await stats.execute("id", {}, undefined, noop, { cwd: process.cwd() });
  const t2 = r2.content?.[0]?.text ?? "";
  assert("memory_stats returns total_chunks", /total_chunks/.test(t2), t2.slice(0, 200));

  const cols = tools.get("collections_list");
  const r3 = await cols.execute("id", {}, undefined, noop, { cwd: process.cwd() });
  const t3 = r3.content?.[0]?.text ?? "";
  assert("collections_list returns an array", t3.trim().startsWith("["), t3.slice(0, 200));

  const ctx = tools.get("memory_context");
  const r4 = await ctx.execute("id", { query: "memnest smoke", n_results: 1 }, undefined, noop, { cwd: process.cwd() });
  const t4 = r4.content?.[0]?.text ?? "";
  if (/memnest error 404/.test(t4)) {
    console.log("  SKIP  memory_context live call (server does not expose /context yet)");
  } else {
    assert("memory_context returns a context pack", /prompt|memnest_context|memories/.test(t4), t4.slice(0, 200));
  }
}

console.log(`\nsmoke: ${ok} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
