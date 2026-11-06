/**
 * pi-memnest task cards
 *
 * A task card is a short handoff note for work that spans several sessions. The working
 * model writes it with task_card save, and it is stored as an ordinary Manual memory whose
 * new version supersedes the old one. Active cards live in one shared collection, so they
 * follow the user across folders, and finished cards move to a separate one, so the
 * session-start search only scans tasks still in progress. A new session lists active cards
 * once, ranked by the first request, and every opened card is pinned into the system
 * prompt on every run, so compaction cannot summarize it away.
 *
 * The user's own messages are appended to the card verbatim by the extension, not by the
 * model. A weaker model can summarize the rest poorly, but it cannot drop or reverse what
 * the user said (measured in docs/task-cards.md).
 */

import { Type } from "typebox";
import { memoryReadAllowed, MODEL_POLICY_ERROR } from "./model-policy.js";

type Call = (
	path: string,
	body?: unknown,
	method?: "GET" | "POST" | "DELETE",
) => Promise<{ text: string; error: boolean }>;
type Card = { id: string; title: string; updated: string };

// The core scans a whole collection on scoped search, so finished cards stay out of the active one.
const PROJECT = "task-cards";
const DONE_PROJECT = "task-cards-done";
const MARK = "[memnest-task-card]";
const ENTRY = "memnest-task-card";
const PIN = "memnest task card: ";
const MAX_CHARS = 12000;
const MAX_OPEN = 3;
const LIST = 20;
const PAGE = 50;
const LOG = "## User's own words (kept verbatim by memnest, oldest first)";
const LOG_MAX = 8000;
const MESSAGE_MAX = 1000;
// Measured on a real long session: with this line the model stopped overriding the user's
// standing instructions with its own analysis far more often (see docs/task-cards.md).
const RULE =
	"The user's stated policies and prohibitions for this task take priority over your own analysis and earlier plans. If a proposal would go against one, first quote the user's words it conflicts with and ask before proposing it. The card's next steps are a plan, not approval: before changing, submitting, publishing, or deleting anything the user has not approved in this conversation, say what you will do and ask. When a user message is about other work than this card, call task_card close for it before answering, so that message stays out of the card.";
// Without this line the model kept using memory_remember for multi-day work and never saved a card (pi session of 2026-11-04).
// The concrete examples matter: with an abstract wording, English requests that resumed yesterday's work got a card 2 of 6 times.
const NO_CARDS =
	"[memnest task cards] No saved task cards were found. Work that spans sessions gets a task card. When the user picks up ongoing work from an earlier day or session (asks for today's next steps, refers to yesterday, or continues a project), or starts work that will go on later, save a card with task_card action=save in this run, as soon as the task is clear; memnest then keeps the user's messages in it verbatim. Single facts still go to memory_remember; the card holds the task's goal, decisions, state, and next steps.";
const FORMAT =
	"Sections: Goal; User policies and prohibitions (dated, close to the user's words, note repeats); Decisions and reasons; Done (what, where); Tried and failed (why); Current state (numbers); Next and waiting; Unverified. Keep what is still valid, move finished items to Done, no tool output, no guesses, under about 6000 characters. Do not copy the user's-own-words section; memnest keeps it.";

// Prompt-time lookups must not hold up the prompt when the core is slow or down.
function within<T>(promise: Promise<T>, fallback: T): Promise<T> {
	return Promise.race([
		promise,
		new Promise<T>((resolve) => (setTimeout(() => resolve(fallback), 1500) as any).unref?.()),
	]);
}

function result(text: string, error = false) {
	return {
		content: [{ type: "text" as const, text: error ? `Error: ${text}` : text }],
		details: undefined,
	};
}

const titleOf = (value: unknown) => String(value ?? "").replace(/\s+/g, " ").trim();
const line = (c: Card) => `- ${c.title} (updated ${c.updated || "unknown"})`;

// With a request, cards are ranked by relevance to it; without one, newest first.
async function findCards(call: Call, request = "", project = PROJECT): Promise<Card[]> {
	// shortcut: search returns at most 50 cards per query and its cost grows with the active collection; add a core list endpoint if active cards reach the thousands.
	const r = await call("/search", {
		// The core drops bracketed queries, so search the bare marker word.
		query: request ? `${ENTRY} ${request.slice(0, 300)}` : ENTRY,
		project,
		n_results: PAGE,
		recent_first: !request,
		adapter: "pi",
	});
	if (r.error) return [];
	try {
		const seen = new Set<string>();
		const cards: Card[] = [];
		for (const item of JSON.parse(r.text).results ?? []) {
			if (typeof item?.id !== "string" || typeof item?.document !== "string") continue;
			if (!item.document.startsWith(MARK)) continue;
			const title = titleOf(item.document.slice(MARK.length).split("\n")[0]);
			if (!title || seen.has(title)) continue;
			seen.add(title);
			cards.push({ id: item.id, title, updated: String(item.timestamp ?? "").slice(0, 16) });
		}
		return cards;
	} catch {
		return [];
	}
}

// Exact title lookup: newest-first page, then a title search for cards past the first page.
async function byTitle(call: Call, title: string, project = PROJECT): Promise<Card | undefined> {
	return (await findCards(call, "", project)).find((c) => c.title === title) ?? (await findCards(call, title, project)).find((c) => c.title === title);
}

async function readCard(call: Call, id: string): Promise<string | null> {
	const r = await call(`/chunk/${encodeURIComponent(id)}?max_chars=${MAX_CHARS + LOG_MAX + 400}`, undefined, "GET");
	if (r.error) return null;
	try {
		const doc = JSON.parse(r.text)?.document;
		return typeof doc === "string" ? doc.split("\n").slice(1).join("\n").trim() : null;
	} catch {
		return null;
	}
}

// A card is the model's body plus the extension's verbatim log of user messages.
function split(text: string): { body: string; log: string[] } {
	const at = text.indexOf(LOG);
	if (at < 0) return { body: text.trim(), log: [] };
	const log = text.slice(at + LOG.length).split("\n").filter((l) => l.startsWith("- "));
	return { body: text.slice(0, at).trim(), log };
}

const TRIMMED = "- (older messages trimmed; memory_search finds them)";

function compose(body: string, log: string[]): string {
	const kept = log.filter((l) => l !== TRIMMED);
	let dropped = kept.length < log.length;
	while (kept.join("\n").length > LOG_MAX && kept.length > 1) {
		kept.shift();
		dropped = true;
	}
	if (!kept.length) return body;
	return `${body}\n\n${LOG}\n${dropped ? `${TRIMMED}\n` : ""}${kept.join("\n")}`;
}

function stamp(): string {
	const d = new Date();
	const p = (n: number) => String(n).padStart(2, "0");
	return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

// pinned: the text last put into the system prompt; kept until a compaction so the prompt cache keeps hitting.
type Open = { title: string; id?: string; body?: string; log?: string[]; pinned?: string };

export function installTaskCards(pi: any, call: Call): void {
	// Opened cards, oldest first; body and log are fetched lazily after a resume.
	let open: Open[] = [];
	// User messages of this session not yet attached to a card.
	let pending: string[] = [];
	let listed = false;
	// Id of the newest compaction entry seen; a new one means the transcript was summarized.
	let lastCompaction: string | undefined;
	const newestCompaction = (ctx: any): string | undefined => {
		const branch = ctx?.sessionManager?.getBranch?.() ?? [];
		for (let i = branch.length - 1; i >= 0; i--) if (branch[i]?.type === "compaction") return branch[i].id;
		return undefined;
	};

	const remember = () => pi.appendEntry?.(ENTRY, { titles: open.map((c) => c.title) });
	const setOpen = (card: Open) => {
		open = [...open.filter((c) => c.title !== card.title), card].slice(-MAX_OPEN);
		remember();
	};
	const close = (title: string) => {
		open = open.filter((c) => c.title !== title);
		remember();
	};
	// Re-reads the newest version right before writing, so another session's saves and
	// messages are kept. shortcut: two writes within the same few milliseconds can still drop one message; give the log its own records if that ever matters.
	// Writes from this session run one at a time, so each one re-reads the previous one's result.
	let writing: Promise<unknown> = Promise.resolve();
	const attachPending = (card: Open, batch = pending.splice(0)) => {
		if (!batch.length) return writing;
		card.log = [...(card.log ?? []), ...batch];
		const write = () => appendLog(card, batch);
		writing = writing.then(write, write);
		return writing;
	};
	// Another session can write between our read and write, so check after writing and
	// merge again into whatever won. The log never loses a message to a concurrent session.
	const appendLog = async (card: Open, batch: string[]) => {
		for (let attempt = 0; attempt < 3; attempt++) {
			const latest = await byTitle(call, card.title);
			const text = latest ? await readCard(call, latest.id) : null;
			if (!latest || text === null) return;
			const fresh = split(text);
			const missing = batch.filter((e) => !fresh.log.includes(e));
			Object.assign(card, { id: latest.id, body: fresh.body, log: [...fresh.log, ...missing] });
			if (!missing.length) return;
			await call("/update", { id: latest.id, text: `${MARK} ${card.title}\n${compose(fresh.body, card.log!)}` });
		}
	};

	pi.on("session_start", (_event: unknown, ctx: any) => {
		listed = false;
		pending = [];
		lastCompaction = newestCompaction(ctx);
		open = [];
		// A resumed session reopens the cards it had open.
		for (const entry of ctx?.sessionManager?.getBranch?.() ?? [])
			if (entry?.type === "custom" && entry.customType === ENTRY) {
				const titles = entry.data?.titles ?? (entry.data?.title ? [entry.data.title] : []);
				if (Array.isArray(titles)) open = titles.filter((t: unknown) => typeof t === "string").map((title: string) => ({ title }));
			}
	});

	pi.on("before_agent_start", async (event: any, ctx: any) => {
		const prompt = event?.messageType === "custom" || typeof event?.prompt !== "string" ? "" : event.prompt.trim();
		// Hosts without agent_settled: earlier runs' messages go to the card still open.
		const previous = open.at(-1);
		if (previous?.id && memoryReadAllowed(ctx)) void attachPending(previous).catch(() => {});
		if (prompt && !prompt.startsWith("/")) pending.push(`- ${stamp()} ${prompt.replace(/\s+/g, " ").slice(0, MESSAGE_MAX)}`);
		if (!memoryReadAllowed(ctx)) return;
		for (const c of open)
			if (c.body === undefined) {
				const card = await within(byTitle(call, c.title), undefined);
				const text = card ? await within(readCard(call, card.id), null) : null;
				Object.assign(c, text === null ? { body: "" } : { id: card!.id, ...split(text) });
			}
		const compaction = newestCompaction(ctx);
		const compacted = compaction !== lastCompaction;
		lastCompaction = compaction;
		const files = event?.systemPromptOptions?.contextFiles;
		if (Array.isArray(files)) {
			// A changed pin rewrites the whole cached conversation, so re-render only new pins and after a compaction
			// (which rewrites the cache anyway); until then the conversation itself holds this session's messages.
			for (const c of open) if (c.body && (c.pinned === undefined || compacted)) c.pinned = `${RULE}\n\n${compose(c.body, c.log ?? [])}`;
			const kept = files.filter((f: any) => !f?.path?.startsWith(PIN));
			files.splice(0, files.length, ...kept, ...open.filter((c) => c.pinned).map((c) => ({ path: `${PIN}${c.title}`, content: c.pinned! })));
		}
		const notes: string[] = [];
		if (compacted && open.length) {
			notes.push(`[memnest task cards] The conversation was just compacted. If decisions or user corrections since the last save of ${open.map((c) => `"${c.title}"`).join(", ")} are missing from the card, save it now.`);
		}
		if (!listed && !open.length) {
			listed = true;
			const [matching, recent] = await Promise.all([
				prompt ? within(findCards(call, prompt), []) : Promise.resolve([]),
				within(findCards(call), []),
			]);
			// Relevance first, then recent cards the ranked search did not return.
			const cards = [...matching, ...recent.filter((c) => !matching.some((m) => m.title === c.title))].slice(0, LIST);
			// A full page means many tasks were never finished; each one adds to this lookup.
			const crowded = recent.length >= PAGE ? `\n${PAGE} or more active cards. When it fits the conversation, ask the user which finished tasks to mark done.` : "";
			if (!cards.length) notes.push(NO_CARDS);
			else
				notes.push(
					`[memnest task cards] Saved handoff cards${matching.length ? ", most relevant to this request first" : ", newest first"}. Open one with task_card action=get when the request is about that task: continuing it or asking about it. A new task that only shares a word or an area with a card is not about it, so answer without opening a card. If the user is continuing something but it is unclear which task, ask. If the request picks up earlier work that has no card here, or starts work that will go on later, save a new card for it with task_card action=save.\n${cards.map(line).join("\n")}${recent[0] ? `\nMost recently saved: ${recent[0].title}` : ""}${crowded}`,
				);
		}
		if (!notes.length) return;
		return { message: { customType: ENTRY, display: false, content: `<system-reminder>\n${notes.join("\n\n")}\n</system-reminder>` } };
	});

	// The message is written once the run settles, so a card the model opened by mistake or closed
	// because the user moved on does not keep it; the newest open card is the task it belongs to.
	pi.on("agent_settled", async (_event: unknown, ctx: any) => {
		const current = open.at(-1);
		// Awaited so a one-shot `pi -p` does not exit mid-write, but capped so a stalled core cannot hold pi busy.
		if (current?.id && memoryReadAllowed(ctx)) await within(attachPending(current).catch(() => {}), undefined);
	});

	pi.registerTool({
		name: "task_card",
		label: "Memory: task card",
		description: `Handoff card for work that continues across sessions or days. Cards are shared across folders. get opens a card and keeps it pinned in your instructions for the rest of the session, up to ${MAX_OPEN} at once, so compaction cannot drop it. While a card is open, memnest appends every user message to it verbatim, so the user's instructions are kept without a save. When the user moves on to unrelated work, call close for that card so later messages are not recorded in it. save writes the newest body and replaces the previous version with the same title; the verbatim user messages carry over. Create a card when a task will continue in a later session. After that, save when work progressed or a decision changed the plan, and when the user stops for the day, not after every message. close unpins a card and stops recording into it. done moves a finished task out of the list; get still opens it and saving it again reopens it. list with text searches cards by topic. If no card list was shown at the start of this session, no cards are saved, so do not call list to look for one. ${FORMAT}`,
		parameters: Type.Object({
			action: Type.Union([Type.Literal("list"), Type.Literal("get"), Type.Literal("save"), Type.Literal("done"), Type.Literal("close")]),
			title: Type.Optional(Type.String({ description: "Short task name; the same title updates the same card." })),
			text: Type.Optional(Type.String({ description: "Card body for save (without the user's-own-words section), or a topic to search for list." })),
		}),
		async execute(_id: string, p: any, _s: unknown, _u: unknown, ctx: any) {
			const title = titleOf(p.title);
			if (p.action === "close") {
				if (!open.some((c) => c.title === title)) return result(`task card "${title}" is not open`, true);
				close(title);
				// Keep the request that led to closing it; it may belong to the card saved next.
				pending = pending.slice(-1);
				return result(`closed task card "${title}"; it is no longer pinned or recorded into`);
			}
			if (p.action === "save" || p.action === "done") {
				if (!title) return result(`${p.action} needs a title`, true);
				let previous = await byTitle(call, title);
				if (p.action === "done") {
					if (!previous) return result(`no active task card titled "${title}"`, true);
					// Moving the record keeps its id and text; only the active collection stays small.
					const moved = await call("/update", { id: previous.id, project: DONE_PROJECT });
					if (moved.error) return result(moved.text, true);
					close(title);
					return result(`marked task card "${title}" done; it no longer appears in the list`);
				}
				// The model may echo the pinned card; the log section is never taken from it.
				const body = split(String(p.text ?? "")).body;
				if (!body) return result("save needs title and text", true);
				if (body.length > MAX_CHARS)
					return result(`card is ${body.length} characters; shorten it below ${MAX_CHARS} and save again`, true);
				if (!previous) {
					// Saving a finished task reopens it: move it back so the new version can supersede it.
					const finished = await byTitle(call, title, DONE_PROJECT);
					if (finished && !(await call("/update", { id: finished.id, project: PROJECT })).error) previous = finished;
				}
				// The newest stored log plus anything this session recorded that is not in it yet.
				const opened = open.find((c) => c.title === title);
				const stored = previous ? split((await readCard(call, previous.id)) ?? "").log : [];
				const log = [...stored, ...[...(opened?.log ?? []), ...pending].filter((e) => !stored.includes(e))];
				const r = await call("/add", {
					text: `${MARK} ${title}\n${compose(body, log)}`,
					project: PROJECT,
					metadata: {
						chunk_type: "manual",
						importance: "decision",
						memory_kind: "record",
						supersedes: previous?.id,
						cwd: ctx?.cwd,
						sensitive: false,
						adapter: "pi",
					},
				});
				if (r.error) return result(r.text, true);
				pending = [];
				// Without an id the card stays pinned, but new messages wait for the next save.
				const id = (() => {
					try {
						return JSON.parse(r.text)?.id as string | undefined;
					} catch {
						return undefined;
					}
				})();
				setOpen({ title, id, body, log, pinned: open.find((c) => c.title === title)?.pinned });
				return result(`saved task card "${title}"${previous ? ` (replaces ${previous.id})` : ""}; it stays pinned for this session`);
			}
			if (!memoryReadAllowed(ctx)) return result(MODEL_POLICY_ERROR, true);
			if (p.action === "list") {
				const topic = String(p.text ?? "").trim();
				const cards = await findCards(call, topic);
				return result(cards.length ? cards.map(line).join("\n") : "no task cards are saved, so do not search again. If this work continues from an earlier session or will go on in a later one, save a card for it now with action=save.");
			}
			const active = await byTitle(call, title);
			const card = active ?? (await byTitle(call, title, DONE_PROJECT));
			if (!card) {
				const near = (await findCards(call, title)).slice(0, 5);
				return result(`no task card titled "${title}"${near.length ? `; closest titles:\n${near.map(line).join("\n")}` : ""}`, true);
			}
			const text = await readCard(call, card.id);
			if (!text) return result("could not read the task card", true);
			const opened: Open = { title, id: card.id, ...split(text) };
			// Only the request that picked the task up is recorded; earlier messages may be other work.
			pending = pending.slice(-1);
			setOpen(opened);
			return result(`${active ? "" : "(this task was marked done; save it again to reopen)\n\n"}${RULE}\n\n${compose(opened.body!, opened.log ?? [])}`);
		},
	});
}
