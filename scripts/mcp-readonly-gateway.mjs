#!/usr/bin/env node

// Read-only gateway in front of the Memnest MCP endpoint, for remote clients
// that cannot send a static Authorization header (see docs/web-connectors.md).
//
// It serves POST /<secret>/mcp, forwards to ${MEMNEST_URL}/mcp with the bearer
// token when one is configured, advertises only the tools in
// MEMNEST_GATEWAY_TOOLS, and refuses tools/call for anything else. Tools that
// write, delete, or touch the vault are refused even when that list names them.
//
// The gateway holds no memory data. It is a filter and a credential holder, so
// the secret path plus the token are the only access control on this hop.

import { createHash, timingSafeEqual } from "node:crypto";
import { createServer } from "node:http";

const secret = (process.env.MEMNEST_GATEWAY_SECRET ?? "").trim();
const port = Number(process.env.MEMNEST_GATEWAY_PORT ?? "3113");
const upstreamBase = (process.env.MEMNEST_URL ?? "http://127.0.0.1:3111").replace(
	/\/$/,
	"",
);
const upstreamUrl = `${upstreamBase}/mcp`;
const token = process.env.MEMNEST_TOKEN?.trim() || undefined;
// This gateway is read-only whatever its configuration says.
const neverAllowed = (name) =>
	name.startsWith("secret_") ||
	["memory_remember", "memory_update", "memory_delete", "task_card"].includes(name);
const allowed = new Set(
	(process.env.MEMNEST_GATEWAY_TOOLS ?? "memory_search,memory_get")
		.split(",")
		.map((name) => name.trim())
		.filter((name) => name && !neverAllowed(name)),
);

if (secret.length < 24) {
	console.error(
		"MEMNEST_GATEWAY_SECRET is missing or shorter than 24 characters; refusing to start",
	);
	process.exit(1);
}
if (allowed.size === 0) {
	console.error("MEMNEST_GATEWAY_TOOLS names no read-only tool; refusing to start");
	process.exit(1);
}

function reply(res, status, body) {
	res.writeHead(status, {
		"content-type": "application/json",
		"cache-control": "no-store",
	});
	res.end(typeof body === "string" ? body : JSON.stringify(body));
}

function refused(id, message) {
	return { jsonrpc: "2.0", id, error: { code: -32601, message } };
}

async function forward(raw) {
	const headers = { "content-type": "application/json" };
	if (token) headers.authorization = `Bearer ${token}`;
	const response = await fetch(upstreamUrl, {
		method: "POST",
		headers,
		body: raw,
		signal: AbortSignal.timeout(120_000),
	});
	return { status: response.status, text: await response.text() };
}

const MAX_BODY_BYTES = 1024 * 1024;

// Hashing first gives equal lengths, so the comparison time does not depend on
// how much of a guessed path segment matches the secret.
const sha256 = (value) => createHash("sha256").update(value).digest();
const secretHash = sha256(secret);

function isGatewayPath(url) {
	let pathname;
	try {
		pathname = new URL(url, "http://localhost").pathname;
	} catch {
		return false;
	}
	const segments = pathname.split("/").filter(Boolean);
	// Checks every segment without stopping at the first match.
	const matched = segments.reduce(
		(found, segment) => timingSafeEqual(sha256(segment), secretHash) || found,
		false,
	);
	return matched && segments.at(-1) === "mcp";
}

// Any throw inside an async request handler would otherwise end the process.
const server = createServer((req, res) => {
	handle(req, res).catch((error) => {
		console.log(`${new Date().toISOString()} request failed: ${error.message}`);
		if (!res.headersSent) reply(res, 400, { error: "bad request" });
		else res.destroy();
	});
});

async function handle(req, res) {
	if (req.method !== "POST") {
		reply(res, 405, { error: "method not allowed" });
		return;
	}
	if (!isGatewayPath(req.url)) {
		reply(res, 404, { error: "not found" });
		return;
	}

	const chunks = [];
	let size = 0;
	for await (const chunk of req) {
		size += chunk.length;
		if (size > MAX_BODY_BYTES) {
			reply(res, 413, { error: "request body too large" });
			req.destroy();
			return;
		}
		chunks.push(chunk);
	}
	const raw = Buffer.concat(chunks).toString("utf8");

	let message;
	try {
		message = JSON.parse(raw);
	} catch {
		reply(res, 400, {
			jsonrpc: "2.0",
			id: null,
			error: { code: -32700, message: "parse error" },
		});
		return;
	}
	if (message === null || typeof message !== "object" || Array.isArray(message)) {
		reply(res, 400, {
			jsonrpc: "2.0",
			id: null,
			error: { code: -32600, message: "expected one JSON-RPC object; batches are not supported" },
		});
		return;
	}

	const method = typeof message.method === "string" ? message.method : "";
	const name = method === "tools/call" ? message?.params?.name : undefined;

	if (method === "tools/call" && !allowed.has(name)) {
		// Tool names are logged; arguments and results are not.
		console.log(`${new Date().toISOString()} refused tools/call ${JSON.stringify(name)}`);
		reply(res, 200, refused(message.id, `tool not available: ${name}`));
		return;
	}
	console.log(`${new Date().toISOString()} ${JSON.stringify(method)}${name ? ` ${JSON.stringify(name)}` : ""}`);

	// evidence_only makes the core run a local model for up to 90 seconds per
	// call; a remote caller does not get to spend that.
	const args = method === "tools/call" ? message.params?.arguments : undefined;
	const stripped = args !== null && typeof args === "object" && "evidence_only" in args;
	if (stripped) delete args.evidence_only;

	let upstream;
	try {
		upstream = await forward(stripped ? JSON.stringify(message) : raw);
	} catch (error) {
		console.log(`${new Date().toISOString()} upstream unavailable: ${error.message}`);
		reply(res, 502, {
			jsonrpc: "2.0",
			id: message.id ?? null,
			error: { code: -32603, message: "memnest is unreachable" },
		});
		return;
	}

	if (method === "tools/list" && upstream.status === 200) {
		try {
			const parsed = JSON.parse(upstream.text);
			if (Array.isArray(parsed?.result?.tools)) {
				parsed.result.tools = parsed.result.tools.filter((tool) =>
					allowed.has(tool.name),
				);
			}
			reply(res, 200, parsed);
			return;
		} catch {
			// Never pass an unfiltered list through when filtering fails.
			reply(res, 502, {
				jsonrpc: "2.0",
				id: message.id ?? null,
				error: { code: -32603, message: "upstream response was not JSON" },
			});
			return;
		}
	}

	reply(res, upstream.status, upstream.text);
}

server.listen(port, "127.0.0.1", () => {
	console.log(
		`read-only MCP gateway on 127.0.0.1:${port} -> ${upstreamUrl}` +
			`, tools: ${[...allowed].join(", ")}` +
			`, bearer: ${token ? "set" : "none"}`,
	);
});
