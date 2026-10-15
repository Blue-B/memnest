#!/usr/bin/env node

// Read-only gateway in front of the Memnest MCP endpoint, for remote clients
// that cannot send a static Authorization header (see docs/web-connectors.md).
//
// It serves POST /<secret>/mcp, forwards to ${MEMNEST_URL}/mcp with the bearer
// token when one is configured, advertises only the tools in
// MEMNEST_GATEWAY_TOOLS, and refuses tools/call for anything else.
//
// The gateway holds no memory data. It is a filter and a credential holder, so
// the secret path plus the token are the only access control on this hop.

import { createServer } from "node:http";

const secret = (process.env.MEMNEST_GATEWAY_SECRET ?? "").trim();
const port = Number(process.env.MEMNEST_GATEWAY_PORT ?? "3113");
const upstreamBase = (process.env.MEMNEST_URL ?? "http://127.0.0.1:3111").replace(
	/\/$/,
	"",
);
const upstreamUrl = `${upstreamBase}/mcp`;
const token = process.env.MEMNEST_TOKEN?.trim() || undefined;
const allowed = new Set(
	(process.env.MEMNEST_GATEWAY_TOOLS ?? "memory_search,memory_get")
		.split(",")
		.map((name) => name.trim())
		.filter(Boolean),
);

if (secret.length < 24) {
	console.error(
		"MEMNEST_GATEWAY_SECRET is missing or shorter than 24 characters; refusing to start",
	);
	process.exit(1);
}
if (allowed.size === 0) {
	console.error("MEMNEST_GATEWAY_TOOLS is empty; refusing to start");
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
	const response = await fetch(upstreamUrl, { method: "POST", headers, body: raw });
	return { status: response.status, text: await response.text() };
}

function isGatewayPath(pathname) {
	const segments = pathname.split("/").filter(Boolean);
	return segments.includes(secret) && segments.at(-1) === "mcp";
}

const server = createServer(async (req, res) => {
	if (req.method !== "POST") {
		reply(res, 405, { error: "method not allowed" });
		return;
	}
	if (!isGatewayPath(new URL(req.url, "http://localhost").pathname)) {
		reply(res, 404, { error: "not found" });
		return;
	}

	let raw = "";
	for await (const chunk of req) raw += chunk;

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
	if (Array.isArray(message)) {
		reply(res, 400, {
			jsonrpc: "2.0",
			id: null,
			error: { code: -32600, message: "batches are not supported" },
		});
		return;
	}

	const method = typeof message.method === "string" ? message.method : "";
	const name = method === "tools/call" ? message?.params?.name : undefined;

	if (method === "tools/call" && !allowed.has(name)) {
		// Tool names are logged; arguments and results are not.
		console.log(`${new Date().toISOString()} refused tools/call ${name}`);
		reply(res, 200, refused(message.id, `tool not available: ${name}`));
		return;
	}
	console.log(`${new Date().toISOString()} ${method}${name ? ` ${name}` : ""}`);

	let upstream;
	try {
		upstream = await forward(raw);
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
});

server.listen(port, "127.0.0.1", () => {
	console.log(
		`read-only MCP gateway on 127.0.0.1:${port} -> ${upstreamUrl}` +
			`, tools: ${[...allowed].join(", ")}` +
			`, bearer: ${token ? "set" : "none"}`,
	);
});
