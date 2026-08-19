#!/usr/bin/env node

import { createInterface } from "node:readline";

const baseUrl = (process.env.MEMNEST_URL ?? "http://127.0.0.1:3111").replace(
	/\/$/,
	"",
);
const token = process.env.MEMNEST_TOKEN?.trim() || undefined;
const defaultAdapter = process.env.MEMNEST_ADAPTER ?? "generic-http";
const timeoutMs = Number(process.env.MEMNEST_TIMEOUT_MS ?? "3000");

export function eventToRequest(event) {
	const adapter = event.adapter ?? defaultAdapter;
	if (event.type === "health") return { method: "GET", path: "/health" };
	if (event.type === "search") {
		if (!event.project) throw new Error("search project is required; use project=all explicitly");
		return {
			method: "POST",
			path: "/search",
			body: {
				query: event.query,
				project: event.project,
				n_results: event.limit ?? 3,
				adapter,
			},
		};
	}
	if (event.type === "feedback") {
		return {
			method: "POST",
			path: "/feedback",
			body: {
				recall_id: event.recall_id,
				memory_id: event.memory_id,
				outcome: event.outcome,
				note: event.note,
			},
		};
	}
	if (event.type === "summary") {
		return {
			method: "POST",
			path: "/summary",
			body: {
				project: event.project ?? "default",
				session_id: event.session_id,
				summary: event.text,
			},
		};
	}
	if (event.type === "remember" || event.type === "message") {
		return {
			method: "POST",
			path: "/add",
			body: {
				project: event.project ?? "default",
				text: event.text,
				metadata: {
					chunk_type: event.type === "message" ? "auto_log" : "manual",
					importance:
						event.importance ??
						(event.type === "message" ? "log" : "knowledge"),
					memory_kind: event.memory_kind ?? "record",
					confidence: event.confidence,
					source_ids: event.source_ids ?? [],
					supersedes: event.supersedes,
					session_id: event.session_id ?? "",
					role: event.role,
					source: event.source,
					cwd: event.cwd,
					adapter,
					adapter_version: event.adapter_version,
				},
			},
		};
	}
	throw new Error(`unsupported event type: ${event.type ?? "missing"}`);
}

export async function sendEvent(event, fetchImpl = fetch) {
	const request = eventToRequest(event);
	const response = await fetchImpl(`${baseUrl}${request.path}`, {
		method: request.method,
		headers: {
			"content-type": "application/json",
			...(token ? { authorization: `Bearer ${token}` } : {}),
		},
		...(request.body ? { body: JSON.stringify(request.body) } : {}),
		signal: AbortSignal.timeout(timeoutMs),
	});
	const text = await response.text();
	if (!response.ok) throw new Error(`memnest ${response.status}: ${text}`);
	try {
		return JSON.parse(text);
	} catch {
		return text;
	}
}

async function main() {
	const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of input) {
		if (!line.trim()) continue;
		try {
			const result = await sendEvent(JSON.parse(line));
			process.stdout.write(`${JSON.stringify({ ok: true, result })}\n`);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: error.message })}\n`,
			);
		}
	}
}

if (import.meta.url === `file://${process.argv[1]}`) main();
