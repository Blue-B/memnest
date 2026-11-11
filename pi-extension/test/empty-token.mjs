#!/usr/bin/env node
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
// The vault token is read from MEMNEST_DATA_DIR; keep the user's own data directory out of the test.
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join as joinPath } from "node:path";
const emptyDataDir = mkdtempSync(joinPath(tmpdir(), "pi-memnest-test-"));
process.env.MEMNEST_DATA_DIR = emptyDataDir;
process.on("exit", () => rmSync(emptyDataDir, { recursive: true, force: true }));

process.env.MEMNEST_TOKEN = "   ";
process.env.MEMNEST_EXPOSE_SECRET_TOOLS = "1";
const requests = [];
globalThis.fetch = async (url, init = {}) => {
	requests.push({ url, init });
	return new Response("[]", { status: 200 });
};
const tools = new Map();
const pi = {
	registerTool(tool) {
		tools.set(tool.name, tool);
	},
	registerCommand() {},
	on() {},
};
const here = dirname(fileURLToPath(import.meta.url));
const extension = await import(
	`${join(here, "..", "dist", "index.mjs")}?empty-token`
);
extension.default(pi);
await tools.get("secret_list").execute("id", {});
assert.equal(requests[0].init.headers.authorization, undefined);
console.log("empty MEMNEST_TOKEN is not sent as bearer authentication");
