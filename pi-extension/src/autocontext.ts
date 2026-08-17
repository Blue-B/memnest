/**
 * pi-memnest autocontext
 *
 * Small, push-based memory recall for the turns where durable memory is most
 * likely to prevent a bad answer. The default `balanced` profile does not dump
 * memory at session start. It only retrieves a tiny card for high-risk prompts
 * such as "we tried this before", credential/account claims, impossibility
 * claims, and money/project strategy questions.
 */

import { Type } from "typebox";

type ExtensionAPI = {
	on: (event: string, handler: (payload?: unknown) => unknown) => void;
	registerTool: (tool: unknown) => void;
};
type GlobalWithProcess = typeof globalThis & {
	process?: { env?: Record<string, string | undefined> };
};

const CUSTOM_TYPE = "memnest-autocontext";
const DISABLE_ENV = "MEMNEST_AUTOCONTEXT_DISABLE";

const env = (globalThis as GlobalWithProcess).process?.env ?? {};
const MEMNEST_URL: string = env.MEMNEST_URL ?? "http://127.0.0.1:3111";
const MEMNEST_TOKEN = env.MEMNEST_TOKEN;
const MODE = String(env.MEMNEST_AUTOCONTEXT_MODE ?? "balanced").toLowerCase();
const N_RESULTS = Math.max(
	1,
	parseInt(env.MEMNEST_AUTOCONTEXT_N || "20", 10) || 20,
);
const TOP_INJECT = Math.max(
	1,
	parseInt(env.MEMNEST_AUTOCONTEXT_TOP || "2", 10) || 2,
);
const MAX_INJECTIONS = Math.max(
	1,
	parseInt(env.MEMNEST_AUTOCONTEXT_MAX_INJECTIONS || "4", 10) || 4,
);
const TOPIC_OVERLAP = Math.max(
	0,
	Math.min(1, Number(env.MEMNEST_AUTOCONTEXT_TOPIC_OVERLAP ?? "0.35")),
);
const MIN_SCORE = Number(env.MEMNEST_AUTOCONTEXT_MIN_SCORE ?? "0.12");
const RISK_MIN_SCORE = Number(env.MEMNEST_AUTOCONTEXT_RISK_MIN_SCORE ?? "0.12");
const MIN_LEN = Math.max(
	1,
	parseInt(env.MEMNEST_AUTOCONTEXT_MIN_LEN || "16", 10) || 16,
);
const TIMEOUT_MS = Math.max(
	200,
	parseInt(env.MEMNEST_AUTOCONTEXT_TIMEOUT_MS || "1500", 10) || 1500,
);
const DOC_CHARS = Math.max(
	80,
	parseInt(env.MEMNEST_AUTOCONTEXT_DOC_CHARS || "240", 10) || 240,
);
const EXCLUDE_PROJECTS = new Set(
	(env.MEMNEST_AUTOCONTEXT_EXCLUDE ?? "_superseded,default,root,global")
		.split(",")
		.map((s: string) => s.trim())
		.filter(Boolean),
);

const TRIVIAL = new Set([
	"ok",
	"okay",
	"응",
	"ㅇㅇ",
	"네",
	"넵",
	"yes",
	"no",
	"아니",
	"고마워",
	"thanks",
	"계속",
	"continue",
	"go",
	"next",
	"stop",
	"그만",
	"ㄱㄱ",
	"동의",
	"맞아",
	"좋아",
	"sure",
	"thx",
	"ty",
	"got it",
	"nice",
	"cool",
	"done",
	"k",
	"kk",
	"yep",
	"yeah",
	"nope",
	"perfect",
	"sounds good",
	"looks good",
	"makes sense",
	"go ahead",
	"keep going",
	"thank you",
]);

// Each rule carries a Korean branch (no word boundaries: Korean is agglutinative
// and \b does not apply between Hangul syllables) and an English branch that
// uses \b so "plan" does not fire on "airplane" and "ads" does not fire on
// "loads". Apostrophes accept both ' and the typographic ’.
const RISK_RULES: Array<{ label: string; re: RegExp }> = [
	{
		label: "memory",
		re: /전에|이전|기억|까먹|잊어|잊었|했었|시도|말했잖|또\s*(말|까먹)|맥락|찾아봤|\b(remember(ed|s)?|recall(ed|s)?|forget(s)?|forgot(ten)?|previous(ly)?|earlier|before|again)\b|\blast\s+(time|session|conversation|chat|week|month)\b|\b(we|you|i)\s+(said|told|discussed|talked|agreed|decided|mentioned|tried)\b|\bas\s+(i|we)\s+(said|mentioned)\b|\bcontext\b/i,
	},
	{
		label: "credential",
		re: /계정|로그인|비밀키|시크릿|api\s*key|토큰|인증|구독|플랜|\b(secrets?|tokens?|oauth|plans?|planning|accounts?|logins?|passwords?|passphrase|credentials?|auth|authn|authz|authentication|authorization|subscriptions?|bearer|2fa|mfa|sso)\b|\blog\s*in\b|\bsign\s*(in|up)\b|\bapi[_\s-]*keys?\b|\b(ssh|private|access|secret)\s+keys?\b/i,
	},
	{
		label: "absence",
		re: /없다|없어|없음|없는|없나요|안\s*되|안됨|불가능|못\s*하|지원\s*안|처음|모르겠|\b(cannot|cant|missing|broken|unavailable|unsupported|impossible|fails?|failed|failing|deprecated)\b|\bcan['’]t\b|\b(does|do|did|doesn['’]t|don['’]t|didn['’]t|isn['’]t|wasn['’]t|won['’]t)\s+(not\s+)?(work|working|exist|support|supported)\b|\bnot\s+(work|working|supported|available|possible|found|exist)\b|\bno\s+longer\b|\bnever\s+work(s|ed)?\b/i,
	},
	{
		label: "money",
		re: /돈|수익|크몽|외주|토스|홍보|광고|매출|과금|프로모션|유료|결제|\b(iap|promotions?|monetiz(e|ed|ing|ation)|revenue|profits?|pricing|prices?|billing|payments?|paid|subscriptions?|marketing|ads?|advertising|churn|conversion|freemium|paywall|refunds?|costs?)\b|\buser\s+(acquisition|growth|retention)\b|유저\s*(획득|유입)|사용자\s*(확보|유입)/i,
	},
	{
		label: "config",
		re: /설정|세팅|셋업|환경\s*변수|옵션|프로필|임계값|기본값|\b(re)?config(s|ure|ured|uring|uration)?\b|\b(settings?|setup|profiles?|thresholds?|defaults?|options?|flags?|parameters?|preferences?|toggles?|ports?|timeouts?)\b|\bset\s+up\b|\benv(ironment)?\s*(vars?|variables?)\b|\.env\b/i,
	},
];

interface MemResult {
	project?: string;
	document?: string;
	score?: number;
}

export function isSubstantive(prompt: string): boolean {
	const t = (prompt || "").trim();
	if (t.length < MIN_LEN) return false;
	if (t.startsWith("/")) return false;
	if (TRIVIAL.has(t.toLowerCase())) return false;
	return true;
}

function normQuery(prompt: string): string {
	return prompt.trim().replace(/\s+/g, " ").toLowerCase().slice(0, 240);
}

// \p{L}/\p{N} instead of an a-z/가-힣 range so Japanese, Chinese, Cyrillic and
// accented Latin (café, über, niño) survive tokenisation for the topic-overlap
// gate. Requires the /u flag; tsconfig target es2022 supports it.
export function topicTokens(prompt: string): Set<string> {
	const raw = prompt.toLowerCase().match(/[\p{L}\p{N}_]{2,}/gu) ?? [];
	const stop = new Set([
		"the",
		"and",
		"for",
		"with",
		"this",
		"that",
		"그거",
		"이거",
		"좀",
		"해줘",
		"하면",
		"그리고",
		"근데",
	]);
	return new Set(raw.filter((t) => !stop.has(t)).slice(0, 80));
}

function overlap(a: Set<string>, b: Set<string>): number {
	if (a.size === 0 || b.size === 0) return 0;
	let inter = 0;
	for (const t of a) if (b.has(t)) inter++;
	return inter / Math.min(a.size, b.size);
}

export function riskLabels(prompt: string): string[] {
	const labels: string[] = [];
	for (const rule of RISK_RULES)
		if (rule.re.test(prompt)) labels.push(rule.label);
	return labels;
}

function shouldRunGeneralLane(): boolean {
	return MODE === "aggressive" || env.MEMNEST_AUTOCONTEXT_GENERAL === "1";
}

function isMemResult(value: unknown): value is MemResult {
	if (!value || typeof value !== "object") return false;
	const r = value as Record<string, unknown>;
	return r.document === undefined || typeof r.document === "string";
}

async function searchMemnest(query: string): Promise<MemResult[]> {
	const ctrl = new AbortController();
	const timer = setTimeout(() => ctrl.abort(), TIMEOUT_MS);
	try {
		const res = await fetch(`${MEMNEST_URL}/search`, {
			method: "POST",
			headers: {
				"content-type": "application/json",
				...(MEMNEST_TOKEN ? { authorization: `Bearer ${MEMNEST_TOKEN}` } : {}),
			},
			// exclude_reserved: server-side drop of root/default/global/_superseded
			// (memnest >= 0.5.1); EXCLUDE_PROJECTS below still covers custom lists.
			body: JSON.stringify({
				query,
				adapter: "pi-autocontext",
				n_results: N_RESULTS,
				exclude_reserved: true,
			}),
			signal: ctrl.signal,
		});
		if (!res.ok) return [];
		const json = (await res.json()) as { results?: unknown };
		return Array.isArray(json.results) ? json.results.filter(isMemResult) : [];
	} catch {
		return [];
	} finally {
		clearTimeout(timer);
	}
}

function formatBlock(
	results: MemResult[],
	reason: string,
	options: { strong: boolean },
): string | null {
	const threshold = options.strong ? RISK_MIN_SCORE : MIN_SCORE;
	const kept = results
		.filter((r) => typeof r.document === "string" && r.document.trim().length > 0)
		.filter((r) => !(r.project && EXCLUDE_PROJECTS.has(r.project)))
		.filter((r) => (typeof r.score === "number" ? r.score : 1) >= threshold)
		.slice(0, TOP_INJECT);
	if (kept.length === 0) return null;

	const lines = kept.map((r, i) => {
		const proj = r.project ? `[${r.project}]` : "";
		const score = typeof r.score === "number" ? ` (${r.score.toFixed(2)})` : "";
		let doc = (r.document || "").replace(/\s+/g, " ").trim();
		if (doc.length > DOC_CHARS) doc = `${doc.slice(0, DOC_CHARS)}…`;
		return `${i + 1}. ${proj}${score} ${doc}`;
	});

	const instruction = options.strong
		? "This was retrieved by a high-risk memory trigger. Apply it if relevant. If you ignore it, explicitly say why."
		: "This is background context. Verify named files and flags before acting. Ignore it if irrelevant.";

	return (
		`<system-reminder>\n` +
		`[memnest-autocontext] Durable memory auto-retrieved (${reason}), ranked by relevance. ${instruction}\n\n` +
		lines.join("\n") +
		`\n</system-reminder>`
	);
}

function riskSearchQuery(prompt: string, labels: string[]): string {
	const hints = [
		"prior decisions",
		"user preferences",
		"corrections",
		"previous attempts",
	];
	if (labels.includes("credential"))
		hints.push("accounts", "credentials", "secret keys");
	if (labels.includes("money"))
		hints.push("profit", "promotion", "failed launches", "monetization");
	return `${prompt}\n${hints.join(" ")}`;
}

export function installAutocontext(pi: ExtensionAPI): void {
	const disabled = env[DISABLE_ENV] === "1" || MODE === "off" || MODE === "none";

	let lastSeenQuery: string | null = null;
	let lastInjectedTokens: Set<string> | null = null;
	let lastInjectedAt = 0;
	let lastInjectedCount = 0;
	let lastSkipReason = disabled ? "disabled" : "none";
	let lastInjectReason = "none";
	let injections = 0;

	pi.on("session_start", () => {
		lastSeenQuery = null;
		lastInjectedTokens = null;
		lastInjectedAt = 0;
		lastInjectedCount = 0;
		lastSkipReason = disabled ? "disabled" : "none";
		lastInjectReason = "none";
		injections = 0;
	});

	if (!disabled) {
		pi.on("before_agent_start", async (event: unknown) => {
			const e =
				event && typeof event === "object" ? (event as { prompt?: unknown }) : {};
			const prompt: string = typeof e.prompt === "string" ? e.prompt : "";
			if (!isSubstantive(prompt)) {
				lastSkipReason = "not-substantive";
				return;
			}

			const q = normQuery(prompt);
			if (q === lastSeenQuery) {
				lastSkipReason = "duplicate-prompt";
				return;
			}
			lastSeenQuery = q;

			if (injections >= MAX_INJECTIONS) {
				lastSkipReason = "session-cap";
				return;
			}

			const labels = riskLabels(prompt);
			let reason = "";
			let query = prompt;
			let strong = false;
			let tokensForSuccess: Set<string> | null = null;

			if (labels.length > 0) {
				reason = `risk:${labels.join(",")}`;
				query = riskSearchQuery(prompt, labels);
				strong = true;
				tokensForSuccess = topicTokens(prompt);
			} else if (shouldRunGeneralLane()) {
				const tokens = topicTokens(prompt);
				reason = "first-substantive-turn";
				if (lastInjectedTokens) {
					const sim = overlap(tokens, lastInjectedTokens);
					if (sim >= TOPIC_OVERLAP) {
						lastSkipReason = `same-topic overlap=${sim.toFixed(2)}`;
						return;
					}
					reason = `topic-shift overlap=${sim.toFixed(2)}`;
				}
				tokensForSuccess = tokens;
			} else {
				lastSkipReason = "no-risk-trigger";
				return;
			}

			const results = await searchMemnest(query);
			const block = formatBlock(results, reason, { strong });
			if (!block) {
				lastSkipReason = "no-results";
				return;
			}

			if (tokensForSuccess) lastInjectedTokens = tokensForSuccess;
			lastInjectedAt = Date.now();
			lastInjectedCount = Math.min(results.length, TOP_INJECT);
			lastInjectReason = reason;
			lastSkipReason = "none";
			injections++;

			return {
				message: { customType: CUSTOM_TYPE, content: block, display: false },
			};
		});
	}

	pi.registerTool({
		name: "memnest_autocontext_status",
		label: "Memnest Autocontext Status",
		description:
			"Inspect pi-memnest autocontext: profile, live retrieval, risk-trigger state, and injection counters.",
		parameters: Type.Object({
			query: Type.Optional(
				Type.String({
					description: "Optional: run a live retrieval and preview the block.",
				}),
			),
		}),
		execute: async (_id: string, params: { query?: string }) => {
			const lines: string[] = [];
			lines.push(`memnest URL          : ${MEMNEST_URL}`);
			lines.push(`disabled             : ${disabled}`);
			lines.push(`mode                 : ${MODE}`);
			lines.push(`n_results / top      : ${N_RESULTS} / ${TOP_INJECT}`);
			lines.push(`max injections       : ${MAX_INJECTIONS}`);
			lines.push(`topic overlap gate   : ${TOPIC_OVERLAP}`);
			lines.push(`min_score general/risk: ${MIN_SCORE} / ${RISK_MIN_SCORE}`);
			lines.push(`min_len / timeout    : ${MIN_LEN} / ${TIMEOUT_MS}ms`);
			lines.push(
				`excluded projects    : ${[...EXCLUDE_PROJECTS].join(", ") || "(none)"}`,
			);
			lines.push(`injections so far    : ${injections}`);
			lines.push(
				`last injection       : ${lastInjectedAt ? new Date(lastInjectedAt).toISOString() : "(never)"} (${lastInjectedCount} items, ${lastInjectReason})`,
			);
			lines.push(`last skip reason     : ${lastSkipReason}`);
			if (params?.query) {
				const query = String(params.query);
				const labels = riskLabels(query);
				const results = await searchMemnest(
					labels.length ? riskSearchQuery(query, labels) : query,
				);
				const block = formatBlock(
					results,
					labels.length
						? `manual-risk-preview:${labels.join(",")}`
						: "manual-status-preview",
					{ strong: labels.length > 0 },
				);
				lines.push("");
				lines.push(`--- live retrieval for: ${query} ---`);
				lines.push(block ?? "(no results above min_score)");
			}
			return { content: [{ type: "text", text: lines.join("\n") }] };
		},
	});
}
