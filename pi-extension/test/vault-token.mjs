#!/usr/bin/env node
// Vault tools against a core that asks for its per-install API token: no token file gives a
// clear error (the core answered 401), a wrong token is a 401, the right token works. The
// token goes to a loopback core only, and a bearer token never travels over plain HTTP to
// another machine.
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const bundle = join(dirname(fileURLToPath(import.meta.url)), "..", "dist", "index.mjs");
const dataDir = mkdtempSync(join(tmpdir(), "pi-memnest-vault-"));
const API_TOKEN = "t".repeat(44);
const requests = [];
globalThis.fetch = async (url, init = {}) => {
	const path = new URL(url).pathname;
	requests.push({ url: String(url), path, authorization: init.headers?.authorization });
	if (path === "/health") return new Response(JSON.stringify({ status: "ok", data_dir: dataDir }), { status: 200 });
	if (init.headers?.authorization !== `Bearer ${API_TOKEN}`) return new Response("", { status: 401 });
	return new Response("[]", { status: 200 });
};
const load = async (tag, env) => {
	delete process.env.MEMNEST_TOKEN;
	delete process.env.MEMNEST_URL;
	Object.assign(process.env, { MEMNEST_EXPOSE_SECRET_TOOLS: "1" }, env);
	const tools = new Map();
	(await import(`${bundle}?${tag}`)).default({ registerTool: (t) => tools.set(t.name, t), registerCommand() {}, on() {} });
	return async (name, args = {}) => (await tools.get(name).execute("id", args)).content[0].text;
};

try {
	const local = await load("vault-local", {});
	// No token file: the core says 401 and the tool explains what is missing.
	const missing = await local("secret_list");
	assert.match(missing, /^Error: The vault needs the core's API token[\s\S]*api-token/);
	assert.equal(requests.at(-1).authorization, undefined);

	// A wrong token is refused by the core, and the file is read again afterwards.
	writeFileSync(join(dataDir, "api-token"), "wrong-token\n");
	assert.match(await local("secret_list"), /^Error: memnest error 401/);
	assert.equal(requests.at(-1).authorization, "Bearer wrong-token");

	writeFileSync(join(dataDir, "api-token"), `${API_TOKEN}\n`);
	for (const [name, args, path] of [
		["secret_list", {}, "/secrets"],
		["secret_get", { key: "a b" }, "/secrets/a%20b"],
		["secret_set", { key: "k", value: "v" }, "/secrets"],
		["secret_delete", { key: "k" }, "/secrets/k"],
	]) {
		assert.equal(await local(name, args), "[]", name);
		assert.equal(requests.at(-1).path, path);
		assert.equal(requests.at(-1).authorization, `Bearer ${API_TOKEN}`);
	}
	// Memory calls never carry the vault token.
	await local("memory_delete", { id: "x" });
	assert.equal(requests.at(-1).authorization, undefined);

	// Another machine: the file is not read and the token is not sent.
	const remote = await load("vault-remote", { MEMNEST_URL: "http://192.168.0.10:3111" });
	const before = requests.length;
	assert.match(await remote("secret_list"), /^Error: The vault needs the core's API token/);
	assert.deepEqual(requests.slice(before).map((r) => [r.path, r.authorization]), [["/secrets", undefined]]);

	// MEMNEST_TOKEN replaces the file, goes over https, and is refused for plain HTTP elsewhere.
	const tls = await load("vault-tls", { MEMNEST_URL: "https://memnest.example.com", MEMNEST_TOKEN: API_TOKEN });
	assert.equal(await tls("secret_list"), "[]");
	assert.equal(requests.at(-1).authorization, `Bearer ${API_TOKEN}`);
	const plain = await load("vault-plain", { MEMNEST_URL: "http://192.168.0.10:3111", MEMNEST_TOKEN: API_TOKEN });
	const sent = requests.length;
	assert.match(await plain("secret_list"), /only sent to a loopback or https MEMNEST_URL/);
	assert.match(await plain("memory_delete", { id: "x" }), /only sent to a loopback or https MEMNEST_URL/);
	assert.equal(requests.length, sent, "nothing leaves over plain HTTP with a token configured");
	console.log("vault token: missing -> clear error, wrong -> 401, right -> ok; loopback only; no bearer over remote plain HTTP");
} finally {
	rmSync(dataDir, { recursive: true, force: true });
}
