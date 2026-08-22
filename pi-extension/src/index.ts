import { installAutocontext } from "./autocontext.js";

import { Type } from "typebox";

type ExtensionAPI = any;
type Context = { cwd?: string };
type Result = {
	content: Array<{ type: "text"; text: string }>;
	details: undefined;
};
const env: Record<string, string | undefined> =
	(globalThis as any).process?.env ?? {};
const URL = (env.MEMNEST_URL ?? "http://127.0.0.1:3111").replace(/\/$/, "");
const TOKEN = env.MEMNEST_TOKEN?.trim() || undefined;
const Empty = Type.Object({});

async function call(
	path: string,
	body?: unknown,
	method: "GET" | "POST" | "DELETE" = "POST",
) {
	try {
		const response = await fetch(`${URL}${path}`, {
			method,
			headers: {
				"content-type": "application/json",
				...(TOKEN ? { authorization: `Bearer ${TOKEN}` } : {}),
			},
			...(body !== undefined && method !== "GET"
				? { body: JSON.stringify(body) }
				: {}),
		});
		const text = await response.text();
		return {
			text: response.ok ? text : `memnest error ${response.status}: ${text}`,
			error: !response.ok,
		};
	} catch (error: any) {
		return {
			text: `memnest unreachable at ${URL}: ${error?.message ?? error}`,
			error: true,
		};
	}
}
function result(text: string, error = false): Result {
	return {
		content: [{ type: "text", text: error ? `Error: ${text}` : text }],
		details: undefined,
	};
}
function registerTool(
	pi: ExtensionAPI,
	name: string,
	label: string,
	description: string,
	parameters: any,
	execute: any,
) {
	pi.registerTool({ name, label, description, parameters, execute });
}

export default function register(pi: ExtensionAPI): void {
	installAutocontext(pi);
	pi.registerCommand?.("memnest", {
		description: "Show Memnest status and dashboard URL",
		handler: async (_args: string, ctx: any) => {
			const [health, stats] = await Promise.all([
				call("/health", undefined, "GET"),
				call("/stats", undefined, "GET"),
			]);
			if (health.error) {
				const message = `Memnest unreachable\nDashboard: ${URL}/`;
				ctx.ui.setStatus("memnest", "Memnest: unreachable");
				ctx.ui.notify(message, "error");
				return;
			}
			const healthData = JSON.parse(health.text);
			const count = stats.error
				? "unavailable"
				: JSON.parse(stats.text).total_chunks;
			const message = `Memnest ok\nMemories: ${count}\nData: ${healthData.data_dir}\nDashboard: ${URL}/`;
			ctx.ui.setStatus("memnest", `Memnest: ok, ${count} memories`);
			ctx.ui.notify(message, "info");
		},
	});

	registerTool(
		pi,
		"memory_remember",
		"Memory: remember",
		"Save durable memory. Sensitive values must use secret_set.",
		Type.Object({
			text: Type.String(),
			project: Type.Optional(Type.String()),
			importance: Type.Optional(
				Type.Union(
					[
						Type.Literal("log"),
						Type.Literal("knowledge"),
						Type.Literal("decision"),
						Type.Literal("preference"),
					],
					{ default: "knowledge" },
				),
			),
			memory_kind: Type.Optional(
				Type.Union(
					[
						Type.Literal("record"),
						Type.Literal("fact"),
						Type.Literal("rule"),
						Type.Literal("procedure"),
					],
					{ default: "record" },
				),
			),
			confidence: Type.Optional(Type.Number({ minimum: 0, maximum: 1 })),
			source_ids: Type.Optional(Type.Array(Type.String())),
			supersedes: Type.Optional(Type.String()),
			sensitive: Type.Optional(
				Type.Boolean({ description: "Must be false; use secret_set." }),
			),
		}),
		async (_id: string, p: any, _s: unknown, _u: unknown, ctx: Context) => {
			const cwd = ctx.cwd?.trim();
			if (!p.project && !cwd)
				return result(
					"current workspace is unavailable; pass project explicitly",
					true,
				);
			const r = await call("/add", {
				text: p.text,
				project: p.project ?? "",
				cwd: p.project ? undefined : cwd,
				metadata: {
					chunk_type: "manual",
					importance: p.importance ?? "knowledge",
					memory_kind: p.memory_kind ?? "record",
					confidence: p.confidence,
					source_ids: p.source_ids ?? [],
					supersedes: p.supersedes,
					cwd,
					sensitive: p.sensitive ?? false,
					adapter: "pi",
				},
			});
			return result(r.text, r.error);
		},
	);

	registerTool(
		pi,
		"memory_search",
		"Memory: search",
		"Hybrid memory search. Omit project to use the current workspace; project=all explicitly searches across projects.",
		Type.Object({
			query: Type.String(),
			project: Type.Optional(Type.String()),
			n_results: Type.Optional(
				Type.Integer({ default: 3, minimum: 1, maximum: 50 }),
			),
			recent_first: Type.Optional(Type.Boolean({ default: false })),
			category: Type.Optional(Type.String()),
		}),
		async (_id: string, p: any, _s: unknown, _u: unknown, ctx: Context) => {
			const cwd = ctx.cwd?.trim();
			if (!p.project && !cwd)
				return result(
					"current workspace is unavailable; pass project explicitly (use project=all for cross-project search)",
					true,
				);
			const body = {
				...p,
				project: p.project ?? "",
				cwd: p.project ? undefined : cwd,
				adapter: "pi",
			};
			const r = await call("/search", body);
			if (r.error) return result(r.text, true);
			try {
				const data = JSON.parse(r.text);
				const lines = [
					`=== memory search results (${p.query}) ===`,
					`recall_id=${data.recall_id}`,
				];
				for (const [i, item] of (data.results ?? []).entries())
					lines.push(
						`[${i + 1}] project=${item.project} score=${Number(item.score).toFixed(4)} id=${item.id}\n    ${item.document}`,
					);
				if (!(data.results ?? []).length) lines.push("no results");
				return result(lines.join("\n"));
			} catch {
				return result(r.text);
			}
		},
	);

	registerTool(
		pi,
		"memory_get",
		"Memory: get",
		"Fetch one memory by id.",
		Type.Object({ id: Type.String() }),
		async (_id: string, p: any) => {
			const r = await call(`/chunk/${encodeURIComponent(p.id)}`, undefined, "GET");
			if (r.error) return result(r.text, true);
			try {
				const c = JSON.parse(r.text);
				return result(
					`id=${c.id} project=${c.project} type=${c.chunk_type} importance=${c.importance} created=${c.timestamp}\n${c.document}`,
				);
			} catch {
				return result(r.text);
			}
		},
	);
	registerTool(
		pi,
		"memory_update",
		"Memory: update",
		"Update one memory and refresh indexes.",
		Type.Object({
			id: Type.String(),
			text: Type.Optional(Type.String()),
			project: Type.Optional(Type.String()),
			importance: Type.Optional(
				Type.Union([
					Type.Literal("log"),
					Type.Literal("knowledge"),
					Type.Literal("decision"),
					Type.Literal("preference"),
				]),
			),
			chunk_type: Type.Optional(
				Type.Union([
					Type.Literal("auto_log"),
					Type.Literal("manual"),
					Type.Literal("filtered"),
					Type.Literal("consolidated"),
				]),
			),
			sensitive: Type.Optional(
				Type.Boolean({ description: "Must be false; use secret_set." }),
			),
		}),
		async (_id: string, p: any) => {
			const body = p.sensitive ? { ...p, metadata: { sensitive: true } } : p;
			const r = await call("/update", body);
			return result(r.text, r.error);
		},
	);
	registerTool(
		pi,
		"memory_delete",
		"Memory: delete",
		"Soft-delete one memory.",
		Type.Object({ id: Type.String() }),
		async (_id: string, p: any) => {
			const r = await call("/delete", { ids: [p.id] });
			return result(r.text, r.error);
		},
	);
	registerTool(
		pi,
		"memory_feedback",
		"Memory: feedback",
		"Record recall telemetry. Ranking changes only when memory_id identifies one returned result.",
		Type.Object({
			recall_id: Type.String(),
			memory_id: Type.Optional(Type.String()),
			outcome: Type.Union([
				Type.Literal("helpful"),
				Type.Literal("harmful"),
				Type.Literal("ignored"),
			]),
			note: Type.Optional(Type.String()),
		}),
		async (_id: string, p: any) => {
			const r = await call("/feedback", p);
			return result(r.text, r.error);
		},
	);

	if (env.MEMNEST_EXPOSE_SECRET_TOOLS === "1") {
		registerTool(
			pi,
			"secret_set",
			"Secret: set",
			"Store an encrypted credential.",
			Type.Object({
				key: Type.String(),
				value: Type.String(),
				kind: Type.Optional(Type.String()),
				note: Type.Optional(Type.String()),
			}),
			async (_id: string, p: any) => {
				const r = await call("/secrets", p);
				return result(r.text, r.error);
			},
		);
		registerTool(
			pi,
			"secret_get",
			"Secret: get",
			"Retrieve and decrypt a credential.",
			Type.Object({ key: Type.String() }),
			async (_id: string, p: any) => {
				const r = await call(
					`/secrets/${encodeURIComponent(p.key)}`,
					undefined,
					"GET",
				);
				return result(r.text, r.error);
			},
		);
		registerTool(
			pi,
			"secret_list",
			"Secret: list",
			"List credential metadata without values.",
			Empty,
			async () => {
				const r = await call("/secrets", undefined, "GET");
				return result(r.text, r.error);
			},
		);
		registerTool(
			pi,
			"secret_delete",
			"Secret: delete",
			"Permanently delete a credential.",
			Type.Object({ key: Type.String() }),
			async (_id: string, p: any) => {
				const r = await call(
					`/secrets/${encodeURIComponent(p.key)}`,
					undefined,
					"DELETE",
				);
				return result(r.text, r.error);
			},
		);
	}
}
