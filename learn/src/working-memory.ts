// Working memory: a scratchpad checklist + per-day append-only log + a
// compaction handoff. Pure string transforms (no fs) so they're easy to test;
// the index module owns the actual file IO. Model adopted from jayzeng/pi-memory.

export interface ScratchItem {
  text: string;
  done: boolean;
}

const ITEM_RE = /^- \[( |x|X)\]\s+(.*)$/;

export function parseScratchpad(content: string): ScratchItem[] {
  const items: ScratchItem[] = [];
  for (const line of content.split("\n")) {
    const m = ITEM_RE.exec(line.trim());
    if (m) items.push({ done: m[1]!.toLowerCase() === "x", text: m[2]!.trim() });
  }
  return items;
}

export function formatScratchpad(items: ScratchItem[]): string {
  if (items.length === 0) return "# Scratchpad\n";
  const lines = items.map((i) => `- [${i.done ? "x" : " "}] ${i.text}`);
  return `# Scratchpad\n${lines.join("\n")}\n`;
}

export type ScratchAction = "add" | "done" | "undo" | "remove";

/** Apply a checklist action by case-insensitive substring match (add appends). */
export function applyScratch(
  items: ScratchItem[],
  action: ScratchAction,
  text: string,
): ScratchItem[] {
  const needle = text.trim().toLowerCase();
  if (action === "add") {
    if (items.some((i) => i.text.toLowerCase() === needle)) return items; // dedup exact
    return [...items, { text: text.trim(), done: false }];
  }
  if (action === "remove") {
    return items.filter((i) => !i.text.toLowerCase().includes(needle));
  }
  const done = action === "done";
  return items.map((i) => (i.text.toLowerCase().includes(needle) ? { ...i, done } : i));
}

/** Render an append-only daily-log entry with a stable, parseable stamp. */
export function formatDailyEntry(content: string, isoTs: string, shortSid: string): string {
  return `<!-- ${isoTs} [${shortSid}] -->\n${content.trim()}`;
}

export function appendDaily(existing: string, entry: string): string {
  const sep = existing.trim() ? "\n\n" : "";
  return existing + sep + entry;
}

/**
 * Build a compaction handoff from open scratchpad items + the tail of today's
 * log, so in-progress context survives a context-window compaction. Returns
 * null when there's nothing worth carrying over.
 */
export function buildHandoff(
  scratchpad: string,
  todayLog: string,
  isoTs: string,
  shortSid: string,
  tailLines = 15,
): string | null {
  const parts: string[] = [];
  const open = parseScratchpad(scratchpad).filter((i) => !i.done);
  if (open.length > 0) {
    parts.push("**Open scratchpad items:**");
    for (const i of open) parts.push(`- [ ] ${i.text}`);
  }
  const log = todayLog.trim();
  if (log) {
    const tail = log.split("\n").slice(-tailLines).join("\n");
    parts.push(`**Recent daily log:**\n${tail}`);
  }
  if (parts.length === 0) return null;
  return [`<!-- HANDOFF ${isoTs} [${shortSid}] -->`, "## Session Handoff", ...parts].join("\n");
}
