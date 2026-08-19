#!/usr/bin/env node

import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

process.env.MEMNEST_AUTOLOG = "1";
process.env.MEMNEST_TOKEN = "test-token";
process.env.MEMNEST_URL = "http://127.0.0.1:3111";

const requests = [];
globalThis.fetch = async (url, init = {}) => {
	requests.push({ url: String(url), init });
	const body = String(url).endsWith("/health")
		? '{"status":"ok","data_dir":"/tmp/memnest"}'
		: '{"total_chunks":7}';
	return new Response(body, { status: 200 });
};

const hooks = new Map();
const commands = new Map();
const pi = {
	registerTool() {},
	registerCommand(name, command) {
		commands.set(name, command);
	},
	on(name, handler) {
		const list = hooks.get(name) ?? [];
		list.push(handler);
		hooks.set(name, list);
	},
};

const here = dirname(fileURLToPath(import.meta.url));
const bundle = join(here, "..", "dist", "index.mjs");
const extension = await import(`${bundle}?auth-test`);
extension.default(pi);

const autologHooks = [
	"input",
	"message_end",
	"tool_execution_end",
	"session_compact",
	"agent_end",
	"session_shutdown",
];
assert.ok(
	autologHooks.every((name) => !hooks.has(name)),
	"AutoLog hooks must stay absent even when MEMNEST_AUTOLOG=1",
);
assert.equal(
	requests.filter((request) => request.url.endsWith("/add")).length,
	0,
	"extension registration must not write transcript data",
);

const command = commands.get("memnest");
assert.ok(command, "/memnest command should register");
const notices = [];
await command.handler("", {
	ui: {
		setStatus() {},
		notify(message) {
			notices.push(message);
		},
	},
});
assert.match(notices[0], /Memories: 7/);
assert.match(notices[0], /Data: \/tmp\/memnest/);
assert.match(notices[0], /Dashboard:/);
assert.ok(
	requests
		.filter((request) => /\/(health|stats)$/.test(request.url))
		.every(
			(request) => request.init.headers.authorization === "Bearer test-token",
		),
	"health and stats requests should carry bearer auth",
);

console.log("pi auth, command, and no-AutoLog assertions passed");
