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
		if (!event.project && !event.cwd)
			throw new Error(
				"search project or cwd is required; use project=all explicitly",
			);
		return {
			method: "POST",
			path: "/search",
			body: {
				query: event.query,
				project: event.project ?? "",
				cwd: event.project ? undefined : event.cwd,
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
	if (event.type === "remember") {
		return {
			method: "POST",
			path: "/add",
			body: {
				project: event.project ?? "",
				cwd: event.project ? undefined : event.cwd,
				text: event.text,
				metadata: {
					chunk_type: "manual",
					importance: event.importance ?? "knowledge",
					memory_kind: event.memory_kind ?? "record",
					confidence: event.confidence,
					source_ids: event.source_ids ?? [],
					supersedes: event.supersedes,
					verified_at: event.verified_at,
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
	// Conversation capture belongs to `memnest watch`, the single transcript
	// path. An adapter that also posted messages or session summaries would
	// store every turn twice.
	if (event.type === "message" || event.type === "summary") {
		throw new Error(
			`${event.type} capture is not an adapter operation; use 'memnest watch'`,
		);
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
