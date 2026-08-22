#!/usr/bin/env node
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const bin = process.env.MEMNEST_BIN ?? "memnest";
const data = mkdtempSync(join(tmpdir(), "memnest-contract-e2e-"));
const child = spawn(bin, ["--mcp", "--data-dir", data], {
	stdio: ["pipe", "pipe", "pipe"],
	env: { ...process.env, MEMNEST_EXPOSE_SECRET_TOOLS: "1" },
});
const pending = new Map();
const stderr = [];
let buffer = "";
let nextId = 0;

child.stderr.on("data", (chunk) => stderr.push(chunk.toString()));
function rejectPending(error) {
	for (const { reject, timer } of pending.values()) {
		clearTimeout(timer);
		reject(error);
	}
	pending.clear();
}
child.on("error", rejectPending);
child.on("exit", (code, signal) => {
	if (pending.size)
		rejectPending(new Error(`MCP exited before responding (code=${code}, signal=${signal})\n${stderr.join("").slice(-2000)}`));
});
child.stdout.on("data", (chunk) => {
	buffer += chunk.toString();
	for (let newline; (newline = buffer.indexOf("\n")) >= 0; ) {
		const line = buffer.slice(0, newline).trim();
		buffer = buffer.slice(newline + 1);
		if (!line) continue;
		const response = JSON.parse(line);
		const request = pending.get(response.id);
		if (!request) continue;
		clearTimeout(request.timer);
		pending.delete(response.id);
		request.resolve(response);
	}
});

function request(method, params = {}, timeoutMs = 120000) {
	const id = ++nextId;
	return new Promise((resolve, reject) => {
		const timer = setTimeout(() => {
			pending.delete(id);
			reject(
				new Error(`MCP timeout for ${method}\n${stderr.join("").slice(-2000)}`),
			);
		}, timeoutMs);
		pending.set(id, { resolve, reject, timer });
		child.stdin.write(
			`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`,
		);
	});
}

function stopChild() {
	return new Promise((resolve) => {
		if (child.exitCode !== null || child.signalCode !== null) return resolve();
		const force = setTimeout(() => child.kill("SIGKILL"), 1000);
		child.once("exit", () => {
			clearTimeout(force);
			resolve();
		});
		child.stdin.end();
		child.kill("SIGTERM");
	});
}

const expected = [
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

try {
	const init = await request("initialize", {
		protocolVersion: "2024-11-05",
		capabilities: {},
	});
	if (init.result?.serverInfo?.name !== "memnest")
		throw new Error("initialize failed");
	const listed = await request("tools/list");
	const names = listed.result.tools.map((tool) => tool.name);
	if (JSON.stringify(names) !== JSON.stringify(expected))
		throw new Error(`unexpected tools: ${names.join(",")}`);
	const remembered = await request("tools/call", {
		name: "memory_remember",
		arguments: {
			project: "contract-e2e",
			text: "canonical contract e2e probe",
		},
	});
	if (remembered.result?.isError)
		throw new Error(remembered.result.content?.[0]?.text);
	const memoryId = JSON.parse(remembered.result.content[0].text).id;
	const omittedScope = await request("tools/call", {
		name: "memory_search",
		arguments: { query: "must fail closed" },
	});
	if (!omittedScope.result?.isError) throw new Error("unscoped search was accepted");
	const alias = await request("tools/call", {
		name: "memory_add",
		arguments: { text: "hidden alias" },
	});
	if (!alias.result?.isError) throw new Error("memory_add alias remains callable");
	const searched = await request("tools/call", {
		name: "memory_search",
		arguments: {
			project: "contract-e2e",
			query: "canonical contract probe",
			n_results: 3,
		},
	});
	const searchText = searched.result?.content?.[0]?.text ?? "";
	if (!searchText.includes("canonical contract e2e probe"))
		throw new Error("search did not find remembered memory");
	if (searchText.includes("one-line stubs")) throw new Error("search returned hidden extra candidates");
	const recallId = searchText.match(/recall_id=(recall_[^\n]+)/)?.[1];
	const mismatch = await request("tools/call", {
		name: "memory_feedback",
		arguments: { recall_id: recallId, memory_id: "not-returned", outcome: "helpful" },
	});
	if (!mismatch.result?.isError) throw new Error("feedback mismatch was accepted");
	const feedback = await request("tools/call", {
		name: "memory_feedback",
		arguments: { recall_id: recallId, memory_id: memoryId, outcome: "helpful" },
	});
	if (feedback.result?.isError) throw new Error(feedback.result.content?.[0]?.text);
	const missingSecret = await request("tools/call", {
		name: "secret_get",
		arguments: { key: "missing-e2e-secret" },
	});
	if (!missingSecret.result?.isError) throw new Error("missing secret did not fail");
	const deleted = await request("tools/call", {
		name: "memory_delete",
		arguments: { id: memoryId },
	});
	if (deleted.result?.isError || !deleted.result.content[0].text.includes(memoryId))
		throw new Error("delete failed");
	console.log("MCP E2E: exact tools, scoped search, feedback, secret error, and delete passed");
} finally {
	await stopChild();
	rmSync(data, { recursive: true, force: true });
}
