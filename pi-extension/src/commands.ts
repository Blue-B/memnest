/**
 * pi-memnest slash commands: /memnest-cards and /memnest-search.
 *
 * They are user-initiated reads on the user's own machine. They run without the model, and
 * nothing they show is put into the model's context or sent to a model, so the provider
 * allowlist (MEMNEST_ALLOWED_MODEL_PROVIDERS) does not apply to them. The vault is not
 * shown by any command. Titles and memory text come from stored data: they are shown as
 * text only, with terminal control characters removed.
 */

import { localTime, type Card, type CardControl } from "./task-card.js";

type Reply = { text: string; error: boolean };
type Call = (path: string, body?: unknown, method?: "GET" | "POST" | "DELETE") => Promise<Reply>;
type Level = "info" | "warning" | "error";
type Deps = { call: Call; url: string; cards: CardControl };

const RESULTS = 10;
const FULL_TEXT_CHARS = 30000;
const HIDDEN_PROJECTS = new Set(["task-cards", "task-cards-done", "_superseded"]);
const COMPLETION_CACHE_MS = 30000;

// Terminal cells a character takes: Hangul, CJK and emoji are two wide.
const cells = (ch: string) => {
	const c = ch.codePointAt(0)!;
	const wide =
		(c >= 0x1100 && c <= 0x115f) || (c >= 0x2e80 && c <= 0xa4cf) || (c >= 0xac00 && c <= 0xd7a3) ||
		(c >= 0xf900 && c <= 0xfaff) || (c >= 0xfe30 && c <= 0xfe6f) || (c >= 0xff00 && c <= 0xff60) ||
		(c >= 0xffe0 && c <= 0xffe6) || (c >= 0x1f300 && c <= 0x1faff) || (c >= 0x20000 && c <= 0x3fffd);
	return wide ? 2 : 1;
};
const columns = () => Math.max(40, Math.min(Number((globalThis as any).process?.stdout?.columns) || 100, 200));

/** One line of at most `max` terminal cells; control characters and line breaks become spaces. */
export function oneLine(value: unknown, max = columns() - 6): string {
	const chars = Array.from(String(value ?? "").replace(/[\s\u0000-\u001f\u007f-\u009f]+/g, " ").trim());
	if (chars.reduce((sum, ch) => sum + cells(ch), 0) <= max) return chars.join("");
	let used = 0;
	let kept = "";
	for (const ch of chars) {
		if (used + cells(ch) > max - 1) break;
		used += cells(ch);
		kept += ch;
	}
	return `${kept.trimEnd()}…`;
}
/** Multi-line text with every control character except newline and tab removed (no terminal escapes from stored data). */
export const plain = (value: unknown): string =>
	String(value ?? "").replace(/\r\n?/g, "\n").replace(/[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g, " ");

/** Display name of a project: workspace ids look like ws_<folder>_<hash>. */
export const shortProject = (project: string): string => project.replace(/^ws_(.+)_[0-9a-f]{16,}$/, "$1");

/** `project:<name>` or `project:all` as the first word picks the scope; otherwise the folder decides. */
export function parseSearch(args: string): { project?: string; query: string } {
	const m = /^\s*project:(\S+)(?:\s+([\s\S]*))?$/i.exec(args);
	return m ? { project: m[1], query: (m[2] ?? "").trim() } : { query: args.trim() };
}

function say(ctx: any, text: string, level: Level = "info") {
	if (ctx.hasUI) return ctx.ui.notify(text, level);
	// Without a UI the text is printed; a JSON event stream keeps stdout for itself.
	const out = ctx.mode === "json" ? process.stderr : process.stdout;
	out.write(`${text}\n`);
}

export function installCommands(pi: any, { call, url, cards }: Deps): void {
	// One line for a failed core call: unreachable, too old for the route, or the core's own error.
	const problem = (reply: Reply) => {
		if (reply.text.startsWith("memnest unreachable")) return `Memnest is not reachable at ${url}`;
		if (/^memnest error (404|405)/.test(reply.text)) return "This Memnest core is too old for that command; update the core";
		return oneLine(reply.text, 200);
	};
	const fail = (ctx: any, reply: Reply | string) => say(ctx, typeof reply === "string" ? reply : problem(reply), "error");

	const cardLine = (n: number, c: Card, finished: boolean) =>
		oneLine(`${n}. ${!finished && cards.isOpen(c.title) ? "[open] " : ""}${c.title}  (saved ${localTime(c.updated)})`);

	// Shows one card; returns true when the list should be read again.
	async function cardMenu(ctx: any, card: Card, finished: boolean): Promise<boolean> {
		const text = await cards.read(card);
		if (text === null) return fail(ctx, "Could not read the task card"), false;
		say(ctx, `${oneLine(card.title, 200)}\nsaved ${localTime(card.updated)}${finished ? " (finished)" : ""}\n\n${plain(text)}`);
		if (finished) return false;
		const open = cards.isOpen(card.title);
		const OPEN = "Open in this session (pin it, record your messages into it)";
		const CLOSE = "Close in this session (stop recording into it)";
		const DONE = "Mark done";
		const choice = await ctx.ui.select(oneLine(card.title), [open ? CLOSE : OPEN, DONE, "Back"]);
		if (choice === OPEN) {
			const got = await cards.open(card);
			if ("error" in got) return fail(ctx, got.error), false;
			say(ctx, `Opened "${oneLine(card.title, 100)}": it is pinned for this session and your messages are recorded into it.${got.dropped ? ` Only 3 cards stay open, so "${oneLine(got.dropped, 100)}" was closed.` : ""}`);
			return true;
		}
		if (choice === CLOSE) {
			const failed = cards.close(card.title);
			say(ctx, failed ?? `Closed "${oneLine(card.title, 100)}": it is no longer pinned or recorded into.`, failed ? "error" : "info");
			return true;
		}
		if (choice === DONE) {
			if (!(await ctx.ui.confirm("Mark done?", `"${oneLine(card.title, 100)}" leaves the active list. Saving it again reopens it.`))) return false;
			const failed = await cards.done(card);
			say(ctx, failed ?? `Marked "${oneLine(card.title, 100)}" done.`, failed ? "error" : "info");
			return true;
		}
		return false;
	}

	pi.registerCommand?.("memnest-cards", {
		description: "List task cards; open, close or finish one (no model involved)",
		handler: async (_args: string, ctx: any) => {
			if (!ctx.hasUI) {
				for (const finished of [false, true]) {
					const got = await cards.list(finished);
					if ("error" in got) return fail(ctx, { text: got.error, error: true });
					say(ctx, [`${finished ? "Finished" : "Active"} task cards (${got.cards.length}${got.cards.length >= 50 ? "+" : ""}), newest first:`, ...got.cards.map((c, i) => `  ${cardLine(i + 1, c, finished)}`)].join("\n"));
				}
				return;
			}
			let finished = false;
			for (;;) {
				const got = await cards.list(finished);
				if ("error" in got) return fail(ctx, { text: got.error, error: true });
				const FINISHED = "Finished cards...";
				const ACTIVE = "Active cards...";
				const labels = got.cards.map((c, i) => cardLine(i + 1, c, finished));
				const title = `${finished ? "Finished" : "Active"} task cards: ${got.cards.length}${got.cards.length >= 50 ? "+" : ""}, newest first${finished ? "" : "; [open] = recorded into by this session"}`;
				const choice: string | undefined = await ctx.ui.select(title, [...labels, finished ? ACTIVE : FINISHED]);
				if (choice === undefined) return;
				if (choice === FINISHED || choice === ACTIVE) {
					finished = !finished;
					continue;
				}
				const index = labels.indexOf(choice);
				if (index >= 0) await cardMenu(ctx, got.cards[index], finished);
			}
		},
	});

	// Project names for argument completion; /stats is asked at most every 30 seconds.
	let names: { at: number; list: string[] } | undefined;
	async function projectNames(): Promise<string[]> {
		if (names && Date.now() - names.at < COMPLETION_CACHE_MS) return names.list;
		const reply = await call("/stats", undefined, "GET");
		let list: string[] = [];
		try {
			if (!reply.error) list = (JSON.parse(reply.text).collections ?? []).map((c: any) => String(c?.name ?? "")).filter((n: string) => n && !HIDDEN_PROJECTS.has(n) && !/\s/.test(n));
		} catch {
			// No completions when /stats is unreadable.
		}
		names = { at: Date.now(), list };
		return list;
	}

	async function showMemory(ctx: any, item: any) {
		const reply = await call(`/chunk/${encodeURIComponent(item.id)}?max_chars=${FULL_TEXT_CHARS}`, undefined, "GET");
		if (reply.error) return fail(ctx, reply);
		let chunk: any;
		try {
			chunk = JSON.parse(reply.text);
		} catch {
			return fail(ctx, "Memnest returned an unreadable reply");
		}
		if (typeof chunk?.document !== "string") return fail(ctx, "Memnest returned an unreadable reply");
		const clipped = chunk.document.length >= FULL_TEXT_CHARS && chunk.next_offset !== undefined && chunk.next_offset !== null;
		say(ctx, `${shortProject(String(item.project))} · ${localTime(String(item.timestamp)).slice(0, 10)} · ${oneLine(item.chunk_type, 40).toLowerCase()} · id ${oneLine(item.id, 80)}\n\n${plain(chunk.document)}${clipped ? `\n\n(first ${FULL_TEXT_CHARS} characters)` : ""}`);
	}

	pi.registerCommand?.("memnest-search", {
		description: "Search memory yourself: /memnest-search [project:<name>|project:all] <question> (no model involved)",
		getArgumentCompletions: async (prefix: string) => {
			if (!prefix || /\s/.test(prefix) || !(prefix.toLowerCase().startsWith("project:") || "project:".startsWith(prefix.toLowerCase()))) return null;
			const items = ["all", ...(await projectNames())].map((n) => ({ value: `project:${n}`, label: `project:${n}` }));
			const matching = items.filter((i) => i.value.toLowerCase().startsWith(prefix.toLowerCase()));
			return matching.length ? matching : null;
		},
		handler: async (args: string, ctx: any) => {
			let { project, query } = parseSearch(args ?? "");
			if (!query && ctx.hasUI) query = ((await ctx.ui.input("Search memory for:", "a question or keywords")) ?? "").trim();
			if (!query) return say(ctx, "Usage: /memnest-search [project:<name>|project:all] <question>", "warning");
			const cwd = ctx.cwd?.trim();
			if (!project && !cwd) return fail(ctx, "The current folder is unknown; use project:<name> or project:all");
			// The same request the model's memory_search sends.
			const reply = await call("/search", { query, project: project ?? "", cwd: project ? undefined : cwd, n_results: RESULTS, adapter: "pi" });
			if (reply.error) return fail(ctx, reply);
			let data: any;
			try {
				data = JSON.parse(reply.text);
			} catch {
				return fail(ctx, "Memnest returned an unreadable reply");
			}
			const items: any[] = Array.isArray(data?.results) ? data.results.slice(0, RESULTS) : [];
			const hint = typeof data?.scope_hint === "string" ? oneLine(data.scope_hint, 400) : "";
			const scope = project ? `project ${oneLine(project, 60)}` : "this folder";
			const row = (item: any, n: number) =>
				oneLine(`${n}. ${shortProject(String(item.project))} · ${localTime(String(item.timestamp)).slice(0, 10)} · ${oneLine(item.chunk_type, 20).toLowerCase()} · ${Number(item.score).toFixed(2)} · ${item.document}`);
			if (!items.length) return say(ctx, `No results for "${oneLine(query, 80)}" in ${scope}.${hint ? `\n${hint}` : ""}`);
			if (!ctx.hasUI) return say(ctx, [`${items.length} results for "${oneLine(query, 80)}" in ${scope}:`, ...items.map((item, i) => `  ${row(item, i + 1)}`), ...(hint ? [hint] : [])].join("\n"));
			const labels = items.map((item, i) => row(item, i + 1));
			for (;;) {
				const choice: string | undefined = await ctx.ui.select(`${items.length} results for "${oneLine(query, 60)}" in ${scope}${hint ? `\n${hint}` : ""}`, labels);
				if (choice === undefined) return;
				const index = labels.indexOf(choice);
				if (index >= 0) await showMemory(ctx, items[index]);
			}
		},
	});
}
