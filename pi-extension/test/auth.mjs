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
	return new Response('{"status":"ok","total_chunks":0}', { status: 200 });
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

for (const handler of hooks.get("session_start") ?? []) {
	await handler({}, { cwd: "/tmp/auth-project", ui: { setStatus() {} } });
}
for (const handler of hooks.get("input") ?? []) {
	await handler(
		{ text: "remember this authenticated event", source: "interactive" },
		{ cwd: "/tmp/auth-project", ui: { setStatus() {} } },
	);
}
await new Promise((resolve) => setTimeout(resolve, 20));

const autolog = requests.find((request) => request.url.endsWith("/add"));
assert.ok(autolog, "AutoLog should POST /add");
assert.equal(autolog.init.headers.Authorization, "Bearer test-token");

const beforeAssistant = requests.length;
for (const handler of hooks.get("message_end") ?? []) {
	await handler({
		message: {
			role: "assistant",
			content: [
				{ type: "toolCall", name: "bash", arguments: { command: "echo fixture" } },
				{ type: "text", text: "final summary" },
			],
		},
	});
}
await new Promise((resolve) => setTimeout(resolve, 20));
const assistantAdds = requests
	.slice(beforeAssistant)
	.filter((request) => request.url.endsWith("/add"));
assert.equal(assistantAdds.length, 1, "assistant text should be saved once");
const assistantBody = JSON.parse(assistantAdds[0].init.body).text;
assert.match(assistantBody, /final summary/);
assert.ok(
	!assistantBody.includes("toolCall") && !assistantBody.includes("echo fixture"),
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
assert.match(notices[0], /Dashboard:/);
assert.ok(
	requests
		.filter(
			(request) =>
				request.url.endsWith("/health") || request.url.endsWith("/stats"),
		)
		.every(
			(request) => request.init.headers.Authorization === "Bearer test-token",
		),
	"status requests should carry bearer auth",
);

console.log("pi auth and command: 5 assertions passed");
