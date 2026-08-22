#!/usr/bin/env node
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

process.env.MEMNEST_URL = "http://memnest.invalid";
process.env.MEMNEST_EXPOSE_SECRET_TOOLS = "1";
const requests = [];
globalThis.fetch = async (url, init = {}) => {
	requests.push({ url: String(url), init });
	return new Response('{"status":"not_found"}', { status: 404 });
};
const tools = new Map();
const pi = {
	registerTool(tool) { tools.set(tool.name, tool); },
	registerCommand() {},
	on() {},
};
const here = dirname(fileURLToPath(import.meta.url));
const extension = await import(`${join(here, "..", "dist", "index.mjs")}?contract-errors`);
extension.default(pi);

const missing = await tools.get("secret_get").execute("id", { key: "missing" });
assert.match(missing.content[0].text, /^Error: memnest error 404:/);
const unscoped = await tools.get("memory_search").execute("id", { query: "unsafe" }, undefined, undefined, {});
assert.match(unscoped.content[0].text, /current workspace is unavailable/);
const beforeExplicit = requests.length;
await tools.get("memory_search").execute("id", { query: "explicit", project: "all", n_results: 2 }, undefined, undefined, {});
assert.equal(requests.length, beforeExplicit + 1);
assert.deepEqual(JSON.parse(requests.at(-1).init.body), {
	query: "explicit",
	project: "all",
	n_results: 2,
	adapter: "pi",
});
console.log("pi contract errors: secret 404 and fail-closed/explicit search passed");
