#!/usr/bin/env node
// The read-only gateway against a fake core: only the secret path is served, write and
// vault tools are refused even when MEMNEST_GATEWAY_TOOLS lists them, and evidence_only
// is removed before a call is forwarded.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const gateway = join(dirname(fileURLToPath(import.meta.url)), "mcp-readonly-gateway.mjs");
const secret = "s".repeat(32);
const tools = ["memory_search", "memory_get", "memory_remember", "memory_update", "memory_delete", "task_card", "secret_get", "secret_list"];

const forwarded = [];
const upstream = createServer(async (req, res) => {
	let raw = "";
	for await (const chunk of req) raw += chunk;
	const message = JSON.parse(raw);
	forwarded.push(message);
	const result = message.method === "tools/list" ? { tools: tools.map((name) => ({ name })) } : { content: [{ type: "text", text: "ok" }] };
	res.writeHead(200, { "content-type": "application/json" });
	res.end(JSON.stringify({ jsonrpc: "2.0", id: message.id, result }));
});
await new Promise((resolve) => upstream.listen(0, "127.0.0.1", resolve));

const free = createServer();
await new Promise((resolve) => free.listen(0, "127.0.0.1", resolve));
const port = free.address().port;
await new Promise((resolve) => free.close(resolve));

const env = { ...process.env, MEMNEST_GATEWAY_SECRET: secret, MEMNEST_GATEWAY_PORT: String(port), MEMNEST_URL: `http://127.0.0.1:${upstream.address().port}`, MEMNEST_GATEWAY_TOOLS: tools.join(","), MEMNEST_TOKEN: "" };
const child = spawn(process.execPath, [gateway], { env, stdio: ["ignore", "pipe", "inherit"] });
try {
	await new Promise((resolve, reject) => {
		child.stdout.once("data", resolve);
		child.once("exit", (code) => reject(new Error(`gateway exited with ${code}`)));
	});
	const post = async (path, body) => {
		const response = await fetch(`http://127.0.0.1:${port}${path}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
		return { status: response.status, body: await response.json() };
	};
	const call = (name, args = {}) => post(`/${secret}/mcp`, { jsonrpc: "2.0", id: 1, method: "tools/call", params: { name, arguments: args } });

	assert.equal((await post(`/${"t".repeat(32)}/mcp`, {})).status, 404, "a wrong secret is not found");
	assert.equal((await post(`/${secret.slice(0, -1)}/mcp`, {})).status, 404, "a prefix of the secret is not found");

	const listed = await post(`/${secret}/mcp`, { jsonrpc: "2.0", id: 1, method: "tools/list" });
	assert.deepEqual(listed.body.result.tools.map((t) => t.name), ["memory_search", "memory_get"], "only read tools are listed");

	for (const name of tools.slice(2)) {
		const before = forwarded.length;
		const refused = await call(name, { key: "x" });
		assert.match(refused.body.error.message, /tool not available/, name);
		assert.equal(forwarded.length, before, `${name} never reaches the core`);
	}

	const searched = await call("memory_search", { query: "port", project: "p", evidence_only: true });
	assert.equal(searched.body.result.content[0].text, "ok");
	assert.deepEqual(forwarded.at(-1).params.arguments, { query: "port", project: "p" }, "evidence_only is removed, the rest is untouched");
	await call("memory_get", { id: "a" });
	assert.deepEqual(forwarded.at(-1).params, { name: "memory_get", arguments: { id: "a" } });
	console.log("PASS read-only gateway: secret path, hard-denied write and vault tools, evidence_only stripped.");
} finally {
	child.kill();
	upstream.close();
}

// A list with nothing but refused tools leaves nothing to serve.
const none = spawn(process.execPath, [gateway], { env: { ...env, MEMNEST_GATEWAY_TOOLS: "memory_remember,secret_get" }, stdio: "ignore" });
assert.equal(await new Promise((resolve) => none.once("exit", resolve)), 1, "refuses to start with only write tools");
