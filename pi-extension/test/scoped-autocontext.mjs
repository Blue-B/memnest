#!/usr/bin/env node
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

process.env.MEMNEST_AUTOCONTEXT_MODE = "aggressive";
const requests = [];
globalThis.fetch = async (_url, init) => {
	requests.push(JSON.parse(init.body));
	return new Response(JSON.stringify({ results: [
		{ id: "same", project: "workspace", document: "workspace memory", score: 1 },
		{ id: "cross", project: "other", document: "cross-project memory", score: 1 },
	] }), { status: 200, headers: { "content-type": "application/json" } });
};
const hooks = new Map();
const pi = { registerTool() {}, on(name, fn) { hooks.set(name, fn); }, registerCommand() {} };
const here = dirname(fileURLToPath(import.meta.url));
const extension = await import(`${join(here, "..", "dist", "index.mjs")}?scoped`);
extension.default(pi);
const before = hooks.get("before_agent_start");
assert.ok(before);
const prompt = "please recall the previous deployment configuration decision";
assert.equal(await before({ prompt }), undefined, "unknown workspace must inject nothing");
assert.equal(requests.length, 0, "unknown workspace must not search all projects");
hooks.get("session_start")({}, { cwd: "/tmp/workspace" });
const injected = await before({ prompt: `${prompt} now` });
assert.equal(requests[0].project, "workspace");
const text = injected?.message?.content ?? "";
assert.match(text, /workspace memory/);
assert.doesNotMatch(text, /cross-project memory/);
console.log("scoped autocontext: unknown workspace skipped and cross-project result excluded");
