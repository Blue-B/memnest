/**
 * pi-palimpsest
 *
 * Bridges pi's tool system to a locally running palimpsest server (default
 * http://127.0.0.1:3111). Only HTTP endpoints that exist in the palimpsest
 * Axum router are exposed here. For richer features (notes, facts, knowledge
 * graph), use the palimpsest MCP stdio mode directly.
 */

import { Type } from "typebox";
import type {
  AgentToolResult,
  ExtensionAPI,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";

const PALIMPSEST_URL = process.env.PALIMPSEST_URL ?? "http://127.0.0.1:3111";

// AutoLog can be disabled via env var if user wants tool-only mode.
const AUTOLOG_ENABLED = (process.env.PALIMPSEST_AUTOLOG ?? "1") !== "0";
// Skip extremely short user messages (likely noise: "y", "ok", "\n").
const AUTOLOG_MIN_USER_LEN = Number(process.env.PALIMPSEST_AUTOLOG_MIN_USER_LEN ?? "3");
// Tool result bodies can be huge — cap them before sending to palimpsest.
const AUTOLOG_MAX_CHARS = Number(process.env.PALIMPSEST_AUTOLOG_MAX_CHARS ?? "8000");

async function call(
  path: string,
  body?: unknown,
  method: "GET" | "POST" | "DELETE" = "POST",
): Promise<{ text: string; isError: boolean }> {
  try {
    const init: RequestInit = { method, headers: { "Content-Type": "application/json" } };
    if (body !== undefined && method !== "GET") init.body = JSON.stringify(body);
    const res = await fetch(`${PALIMPSEST_URL}${path}`, init);
    const text = await res.text();
    if (!res.ok) return { text: `palimpsest error ${res.status}: ${text}`, isError: true };
    return { text, isError: false };
  } catch (e: any) {
    return {
      text: `palimpsest unreachable at ${PALIMPSEST_URL}: ${e?.message ?? e}. Check: systemctl --user status palimpsest`,
      isError: true,
    };
  }
}

/**
 * Wrap a string result in the AgentToolResult shape pi expects.
 * AgentToolResult has no `isError` field — errors are surfaced as text prefix.
 */
function textResult(text: string, isError = false): AgentToolResult<undefined> {
  return {
    content: [{ type: "text", text: isError ? `Error: ${text}` : text }],
    details: undefined,
  };
}

function inferProject(cwd?: string): string {
  if (!cwd) return "default";
  const parts = cwd.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || "default";
}

// In-flight tracker so we can drain pending writes before pi exits.
const inFlight = new Set<Promise<unknown>>();

// Fire-and-forget POST — never blocks the agent loop, never throws.
// We DO track the promise so a graceful shutdown can wait for it briefly.
function fireAndForget(path: string, body: unknown): void {
  try {
    const p = fetch(`${PALIMPSEST_URL}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }).catch(() => {});
    inFlight.add(p);
    p.finally(() => inFlight.delete(p));
  } catch {
    /* swallow */
  }
}

async function drainInFlight(timeoutMs = 3000): Promise<void> {
  if (inFlight.size === 0) return;
  await Promise.race([
    Promise.allSettled([...inFlight]),
    new Promise<void>((resolve) => setTimeout(resolve, timeoutMs)),
  ]);
}

/**
 * Extract a flat text representation from an AgentMessage content array.
 * Handles user (string | content[]), assistant (text/thinking/toolCall), and toolResult.
 */
function messageToText(message: any): string {
  if (!message) return "";
  const content = message.content;
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  const parts: string[] = [];
  for (const c of content) {
    if (!c || typeof c !== "object") continue;
    switch (c.type) {
      case "text":
        if (typeof c.text === "string") parts.push(c.text);
        break;
      case "thinking":
        // Skip thinking content — it's verbose internal reasoning, not the actual answer.
        break;
      case "image":
        parts.push(`[image ${c.mimeType ?? ""}]`);
        break;
      case "toolCall":
        try {
          const args = c.arguments ?? c.input ?? {};
          const argStr = typeof args === "string" ? args : JSON.stringify(args).slice(0, 400);
          parts.push(`[toolCall ${c.name ?? "?"}(${argStr})]`);
        } catch {
          parts.push(`[toolCall ${c.name ?? "?"}]`);
        }
        break;
      default:
        break;
    }
  }
  return parts.join("\n").trim();
}

function truncate(s: string, max: number): { text: string; truncated: boolean } {
  if (s.length <= max) return { text: s, truncated: false };
  return { text: s.slice(0, max) + `\n…[truncated ${s.length - max} chars]`, truncated: true };
}

/**
 * Install lifecycle hooks that mirror opencode-plugin behavior:
 *   - message_end (role=user)         → auto_log chunk, source=pi.chat.message
 *   - message_end (role=assistant)    → auto_log chunk, source=pi.text.complete
 *   - tool_execution_end              → auto_log chunk, source=pi.tool.execute.after
 *   - session_compact                 → consolidated summary chunk
 *
 * Disabled when PALIMPSEST_AUTOLOG=0.
 */
function installAutoLog(pi: ExtensionAPI): void {
  if (!AUTOLOG_ENABLED) return;

  // We need cwd → project mapping. ExtensionContext is passed to tool execute()
  // but NOT to event handlers, so we capture it at session_start.
  let cwd: string = process.cwd();
  let sessionId: string = `pi-${Date.now().toString(36)}`;

  pi.on("session_start", () => {
    // process.cwd() at session_start is the right project root for pi sessions.
    cwd = process.cwd();
    sessionId = `pi-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  });

  // User input — use `input` event because `message_end` does not fire for the
  // initial CLI prompt in `pi -p "..."` print mode.
  pi.on("input", (event) => {
    try {
      const e = event as any;
      const text: string = typeof e.text === "string" ? e.text : "";
      if (!text || text.length < AUTOLOG_MIN_USER_LEN) return;
      const { text: clipped, truncated } = truncate(text, AUTOLOG_MAX_CHARS);
      fireAndForget("/add", {
        project: "root",
        text: `User said: ${clipped}`,
        metadata: {
          chunk_type: "auto_log",
          importance: "log",
          session_id: sessionId,
          source: "pi.chat.message",
          role: "user",
          input_source: e.source ?? "unknown",
          truncated,
          cwd,
        },
      });
    } catch {
      /* swallow */
    }
  });

  // Assistant final message only — user side is covered by the `input` hook.
  pi.on("message_end", (event) => {
    try {
      const msg = (event as any).message;
      if (!msg || msg.role !== "assistant") return;

      const body = messageToText(msg);
      if (!body) return;
      const { text: clipped, truncated } = truncate(body, AUTOLOG_MAX_CHARS);

      fireAndForget("/add", {
        project: "root",
        text: `Assistant answered: ${clipped}`,
        metadata: {
          chunk_type: "auto_log",
          importance: "log",
          session_id: sessionId,
          source: "pi.text.complete",
          role: "assistant",
          model: msg.model,
          stop_reason: msg.stopReason,
          truncated,
          cwd,
        },
      });
    } catch {
      /* swallow */
    }
  });

  pi.on("tool_execution_end", (event) => {
    try {
      const e = event as any;
      const toolName: string = e.toolName ?? "unknown";
      // Don't log palimpsest's own tools — would create infinite-ish chatter.
      if (toolName.startsWith("memory_") || toolName.startsWith("secret_") ||
          toolName.startsWith("notes_") || toolName === "collections_list") {
        return;
      }
      const result = e.result;
      let resultText: string;
      if (typeof result === "string") {
        resultText = result;
      } else if (result && Array.isArray(result.content)) {
        resultText = result.content
          .map((c: any) => (c?.type === "text" ? c.text : ""))
          .filter(Boolean)
          .join("\n");
      } else {
        try { resultText = JSON.stringify(result).slice(0, AUTOLOG_MAX_CHARS); }
        catch { resultText = String(result); }
      }
      if (!resultText) return;
      const { text: clipped, truncated } = truncate(resultText, AUTOLOG_MAX_CHARS);
      const label = e.isError ? "Tool error" : "Tool result";

      fireAndForget("/add", {
        project: "root",
        text: `${label} (${toolName}): ${clipped}`,
        metadata: {
          chunk_type: "auto_log",
          importance: e.isError ? "log" : "knowledge",
          session_id: sessionId,
          source: "pi.tool.execute.after",
          tool: toolName,
          is_error: !!e.isError,
          truncated,
          cwd,
        },
      });
    } catch {
      /* swallow */
    }
  });

  // Drain pending HTTP writes before pi exits (esp. in `pi -p` print mode
  // where the process exits immediately after the assistant's final message).
  pi.on("agent_end", async () => {
    await drainInFlight(2000);
  });
  pi.on("session_shutdown", async () => {
    await drainInFlight(2000);
  });

  pi.on("session_compact", (event) => {
    try {
      const e = event as any;
      const summary: string | undefined = e?.compaction?.summary ?? e?.summary;
      if (!summary) return;
      const { text: clipped } = truncate(summary, AUTOLOG_MAX_CHARS * 2);
      fireAndForget("/add", {
        project: "root",
        text: `Session summary: ${clipped}`,
        metadata: {
          chunk_type: "session_summary",
          importance: "knowledge",
          session_id: sessionId,
          source: "pi.session.compact",
          cwd,
        },
      });
    } catch {
      /* swallow */
    }
  });
}

// Collections that are reserved for autolog / noise. Manual lessons MUST NOT land here.
const RESERVED_AUTOLOG_PROJECTS = new Set(["root", "default", "global"]);

/**
 * Decide where a manual memory chunk should be stored.
 *
 * Rules (matches ~/.pi/agent/AGENTS.md "Collection convention"):
 *   1. importance=preference|decision  -> "playbook"   (cross-project knowledge, always)
 *   2. explicit project passed         -> honor it      (unless it's a reserved autolog bucket)
 *   3. importance=knowledge|log + cwd  -> cwd basename  (project-scoped)
 *   4. nothing usable                  -> "playbook"   (safer than dumping into default)
 */
function resolveProject(args: any, ctx: any): string {
  const explicit = (args.project ?? "").toString().trim();
  const imp = args.importance ?? "knowledge";

  if (imp === "preference" || imp === "decision") return "playbook";
  if (explicit && !RESERVED_AUTOLOG_PROJECTS.has(explicit)) return explicit;

  const inferred = inferProject(ctx?.cwd);
  if (inferred !== "default") return inferred;

  return "playbook";
}

const EmptyParams = Type.Object({});

export default function register(pi: ExtensionAPI): void {
  // Install AutoLog event hooks first so they apply for the whole session.
  installAutoLog(pi);

  // ─── memory_remember (POST /add) ─────────────────────────────────────────
  pi.registerTool({
    name: "memory_remember",
    label: "Memory: remember",
    description:
      "Save a memory chunk to palimpsest. Call this proactively whenever you discover something " +
      "reusable across future sessions: project ports/paths, configuration choices, fixes installed, " +
      "user preferences, corrections, gotchas. Persists in ~/.palimpsest/ and is shared with any " +
      "other client (opencode, Claude Code, etc.) pointing at the same palimpsest server. " +
      "Auto-routing: importance=preference|decision -> 'playbook' collection (cross-project knowledge). " +
      "Other importance values land in the current project bucket (cwd basename) or 'playbook' if none. " +
      "Reserved buckets ('root','default','global') are rejected for manual writes.",
    parameters: Type.Object({
      text: Type.String({ description: "Free-form memory content. Be specific and self-contained." }),
      project: Type.Optional(
        Type.String({
          description:
            "Project bucket. Usually omit — auto-routed by importance + cwd. Pass explicitly only for project-scoped knowledge/log when cwd is wrong.",
        }),
      ),
      importance: Type.Optional(
        Type.Union(
          [
            Type.Literal("log"),
            Type.Literal("knowledge"),
            Type.Literal("decision"),
            Type.Literal("preference"),
          ],
          {
            description:
              "log=routine activity, knowledge=fact (default), decision=deliberate choice (-> playbook), preference=user pref/correction/gotcha (-> playbook)",
          },
        ),
      ),
    }),
    async execute(
      _toolCallId: string,
      params: any,
      _signal: AbortSignal | undefined,
      _onUpdate: unknown,
      ctx: ExtensionContext,
    ) {
      const project = resolveProject(params, ctx);
      const r = await call("/add", {
        project,
        text: params.text,
        metadata: { chunk_type: "manual", importance: params.importance ?? "knowledge" },
      });
      return textResult(`[saved to '${project}'] ${r.text}`, r.isError);
    },
  });

  // ─── memory_search (POST /search) ────────────────────────────────────────
  pi.registerTool({
    name: "memory_search",
    label: "Memory: search",
    description:
      "Hybrid BM25+vector search over palimpsest memory. Call at the START of any task touching " +
      "a previously-discussed project, service, or tool — before guessing config paths or rerunning " +
      "discovery commands.",
    parameters: Type.Object({
      query: Type.String({ description: "Natural language query." }),
      project: Type.Optional(Type.String({ description: "Restrict to project bucket. Omit for all." })),
      n_results: Type.Optional(Type.Integer({ default: 10, minimum: 1, maximum: 50 })),
    }),
    async execute(_toolCallId: string, params: any) {
      const body: any = { query: params.query, n_results: params.n_results ?? 10 };
      if (params.project) body.project = params.project;
      const r = await call("/search", body);
      return textResult(r.text, r.isError);
    },
  });

  // ─── memory_stats (GET /stats) ───────────────────────────────────────────
  pi.registerTool({
    name: "memory_stats",
    label: "Memory: stats",
    description:
      "Palimpsest server statistics (total_chunks, total_sessions, total_facts, total_notes).",
    parameters: EmptyParams,
    async execute() {
      const r = await call("/stats", undefined, "GET");
      return textResult(r.text, r.isError);
    },
  });

  // ─── memory_sessions (GET /sessions) ─────────────────────────────────────
  pi.registerTool({
    name: "memory_sessions",
    label: "Memory: sessions",
    description: "List recent session summaries stored in palimpsest.",
    parameters: EmptyParams,
    async execute() {
      const r = await call("/sessions", undefined, "GET");
      return textResult(r.text, r.isError);
    },
  });

  // ─── memory_facts_list (GET /facts) ──────────────────────────────────────
  pi.registerTool({
    name: "memory_facts_list",
    label: "Memory: facts",
    description:
      "List structured facts (subject-predicate-object triples) from the palimpsest knowledge graph.",
    parameters: EmptyParams,
    async execute() {
      const r = await call("/facts", undefined, "GET");
      return textResult(r.text, r.isError);
    },
  });

  // ─── notes_list (GET /notes) ─────────────────────────────────────────────
  pi.registerTool({
    name: "notes_list",
    label: "Notes: list",
    description: "List all palimpsest key-value notes.",
    parameters: EmptyParams,
    async execute() {
      const r = await call("/notes", undefined, "GET");
      return textResult(r.text, r.isError);
    },
  });

  // ─── secret_set (POST /secrets) ──────────────────────────────────────────
  pi.registerTool({
    name: "secret_set",
    label: "Secret: set",
    description:
      "Store a credential (PAT, API key, password) AES-GCM encrypted in palimpsest. Plain value is only returned via secret_get.",
    parameters: Type.Object({
      key: Type.String(),
      value: Type.String(),
      kind: Type.Optional(
        Type.String({ description: "free-form classifier e.g. github_pat, openai_key" }),
      ),
      note: Type.Optional(Type.String()),
    }),
    async execute(_toolCallId: string, params: any) {
      const r = await call("/secrets", {
        key: params.key,
        value: params.value,
        kind: params.kind,
        note: params.note,
      });
      return textResult(r.text, r.isError);
    },
  });

  // ─── secret_get (GET /secrets/:key) ──────────────────────────────────────
  pi.registerTool({
    name: "secret_get",
    label: "Secret: get",
    description: "Retrieve and decrypt a stored credential by key.",
    parameters: Type.Object({ key: Type.String() }),
    async execute(_toolCallId: string, params: any) {
      const r = await call(`/secrets/${encodeURIComponent(params.key)}`, undefined, "GET");
      return textResult(r.text, r.isError);
    },
  });

  // ─── secret_list (GET /secrets) ──────────────────────────────────────────
  pi.registerTool({
    name: "secret_list",
    label: "Secret: list",
    description: "List stored credential keys (values NEVER returned).",
    parameters: EmptyParams,
    async execute() {
      const r = await call("/secrets", undefined, "GET");
      return textResult(r.text, r.isError);
    },
  });
  pi.registerTool({
    name: "secret_delete",
    label: "Secret: delete",
    description:
      "Permanently delete a stored credential by key. Irreversible. Pair with secret_list to confirm the key first.",
    parameters: Type.Object({
      key: Type.String({ description: "Exact key returned by secret_list." }),
    }),
    async execute(_toolCallId, params) {
      const r = await call(`/secrets/${encodeURIComponent(params.key)}`, undefined, "DELETE");
      return textResult(r.text, r.isError);
    },
  });

  pi.registerTool({
    name: "collections_list",
    label: "Collections: list",
    description:
      "List all palimpsest project collections (buckets) with their chunk counts and metadata. " +
      "Use this to discover which projects already have memory recorded before searching.",
    parameters: EmptyParams,
    async execute() {
      const r = await call("/collections", undefined, "GET");
      return textResult(r.text, r.isError);
    },
  });

  pi.registerTool({
    name: "memory_health",
    label: "Memory: health",
    description:
      "Check whether the palimpsest server is reachable and responsive. Returns server liveness. " +
      "Useful as a first call when memory tools start failing.",
    parameters: EmptyParams,
    async execute() {
      const r = await call("/health", undefined, "GET");
      return textResult(r.text, r.isError);
    },
  });
}
