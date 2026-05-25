// Minimal YAML frontmatter helpers. We stay scalar-only on purpose so we
// can ship without `js-yaml`. Values are JSON-encoded when not a safe
// bareword/string, which round-trips reliably and remains human-readable.

const SAFE_KEY = /^[a-zA-Z_][a-zA-Z0-9_]*$/;
const BAREWORD = /^[A-Za-z0-9_\-./@:+]+$/;

export function stringifyFrontmatter(obj) {
  const lines = ["---"];
  for (const [k, v] of Object.entries(obj)) {
    if (!SAFE_KEY.test(k)) throw new Error(`unsafe yaml key: ${k}`);
    lines.push(`${k}: ${encodeValue(v)}`);
  }
  lines.push("---", "");
  return lines.join("\n");
}

function encodeValue(v) {
  if (v === null || v === undefined) return "null";
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  if (Array.isArray(v)) return JSON.stringify(v);
  if (typeof v === "object") return JSON.stringify(v);
  const s = String(v);
  if (s === "") return '""';
  // Multi-line — use JSON to preserve fidelity (round-trippable)
  if (s.includes("\n") || s.includes('"') || !BAREWORD.test(s) || /^(true|false|null|yes|no)$/i.test(s)) {
    return JSON.stringify(s);
  }
  return s;
}

export function parseFrontmatter(text) {
  if (!text.startsWith("---\n") && !text.startsWith("---\r\n")) {
    return { data: {}, body: text };
  }
  const end = text.indexOf("\n---", 4);
  if (end < 0) return { data: {}, body: text };
  const block = text.slice(4, end);
  const rest = text.slice(end + 4).replace(/^\r?\n/, "");
  const data = {};
  for (const line of block.split(/\r?\n/)) {
    if (!line.trim()) continue;
    const i = line.indexOf(":");
    if (i < 0) continue;
    const k = line.slice(0, i).trim();
    const raw = line.slice(i + 1).trim();
    data[k] = decodeValue(raw);
  }
  return { data, body: rest };
}

function decodeValue(raw) {
  if (raw === "null") return null;
  if (raw === "true") return true;
  if (raw === "false") return false;
  if (raw.startsWith('"') || raw.startsWith("[") || raw.startsWith("{")) {
    try { return JSON.parse(raw); } catch { return raw; }
  }
  if (/^-?\d+(\.\d+)?$/.test(raw)) return Number(raw);
  return raw;
}
