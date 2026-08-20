// Core export/import between memnest sqlite and a git-friendly
// markdown tree. Everything here is pure-ish (no shell, no network) so
// the same logic is testable headlessly.

import { mkdir, readFile, writeFile, rm, readdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { createHash } from "node:crypto";
import { openDB } from "./db.mjs";
import { stringifyFrontmatter, parseFrontmatter } from "./yaml.mjs";

const SAFE_NAME = /[^a-zA-Z0-9._-]/g;
function safeName(s) {
  return String(s ?? "unknown").replace(SAFE_NAME, "_").slice(0, 80) || "unknown";
}

function chunkRelPath(chunk) {
  const proj = safeName(chunk.project);
  const id = safeName(chunk.id);
  return `chunks/${proj}/${id}.md`;
}
function factRelPath(fact) {
  // facts table has no project; subject can be huge — hash for filename
  const h = createHash("sha256").update(fact.id || fact.subject).digest("hex").slice(0, 16);
  return `facts/${h}.md`;
}
function noteRelPath(note) {
  return `notes/${safeName(note.key)}.md`;
}
function secretRelPath(secret) {
  return `secrets/${safeName(secret.key)}.enc.md`;
}
function sessionRelPath(sess) {
  const proj = safeName(sess.project);
  const id = safeName(sess.id);
  return `sessions/${proj}/${id}.md`;
}

function chunkToMd(chunk) {
  let metadata = {};
  try { metadata = JSON.parse(chunk.metadata || "{}"); } catch {}
  // Strip noisy raw_chunk and large keyword arrays from frontmatter; keep
  // them out of git history. They remain in the DB.
  const { raw_chunk: _r, keywords: _k, ...meta } = metadata;
  const fm = {
    id: chunk.id,
    project: chunk.project,
    chunk_type: meta.chunk_type ?? null,
    importance: meta.importance ?? null,
    session_id: meta.session_id ?? null,
    source: meta.source ?? null,
    role: meta.role ?? null,
    tool: meta.tool ?? null,
    sensitive: meta.sensitive ?? false,
    created_at: chunk.created_at,
    updated_at: chunk.updated_at,
  };
  return stringifyFrontmatter(fm) + (chunk.document ?? "") + "\n";
}

function factToMd(fact) {
  const fm = {
    id: fact.id,
    subject: fact.subject,
    predicate: fact.predicate,
    timestamp: fact.timestamp,
    source_session: fact.source_session,
  };
  return stringifyFrontmatter(fm) + (fact.object ?? "") + "\n";
}

function noteToMd(note) {
  const fm = { key: note.key, updated: note.updated };
  return stringifyFrontmatter(fm) + (note.value ?? "") + "\n";
}

// memnest encrypts secret values as `$enc$` + base64(nonce||ciphertext||tag)
// with AES-256-GCM (see core/src/crypto.rs). A row that does not match that
// shape is a legacy, hand-edited or corrupt value and may be plaintext, so
// the exporter refuses to write it.
const ENC_PREFIX = "$enc$";
const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;
const MIN_ENC_BYTES = 12 + 16; // GCM nonce + auth tag, before any payload

export function isEncryptedSecret(value) {
  if (typeof value !== "string" || !value.startsWith(ENC_PREFIX)) return false;
  const payload = value.slice(ENC_PREFIX.length);
  if (!BASE64_RE.test(payload)) return false;
  return Buffer.from(payload, "base64").length >= MIN_ENC_BYTES;
}

function assertSecretsEncrypted(rows) {
  // Keys only — a value that failed validation may be plaintext and must
  // never reach a log line, an error message or a file.
  const bad = rows.filter((s) => !isEncryptedSecret(s.value)).map((s) => String(s.key));
  if (!bad.length) return;
  const shown = bad.slice(0, 5).join(", ") + (bad.length > 5 ? `, +${bad.length - 5} more` : "");
  throw new Error(
    `refusing to export: ${bad.length} secret row(s) are not AES-256-GCM ciphertext (keys: ${shown}). ` +
    "Exporting them could commit plaintext. Re-set them through the memnest API so they are encrypted, " +
    "or drop --include-secrets.",
  );
}

function secretToMd(secret) {
  // Callers must run assertSecretsEncrypted first: only validated `$enc$`
  // ciphertext reaches this function, so the frontmatter claim below is
  // true by construction. The file is safe to commit *iff* master.key is
  // not in the repo (we enforce that via .gitignore).
  const fm = {
    key: secret.key,
    kind: secret.kind,
    note: secret.note,
    updated: secret.updated,
    encryption: "aes-256-gcm",
  };
  return stringifyFrontmatter(fm) + (secret.value ?? "") + "\n";
}

function sessionToMd(sess) {
  const fm = {
    id: sess.id,
    project: sess.project,
    session_id: sess.session_id,
    created_at: sess.created_at,
  };
  return stringifyFrontmatter(fm) + (sess.summary ?? "") + "\n";
}

async function writeIfChanged(path, content) {
  await mkdir(join(path, ".."), { recursive: true });
  if (existsSync(path)) {
    const cur = await readFile(path, "utf8");
    if (cur === content) return false;
  }
  await writeFile(path, content);
  return true;
}

export async function exportAll({ dbPath, repoDir, since = null, projects = null, includeSensitive = false, includeSecrets = false }) {
  const db = await openDB(dbPath, { readonly: true });
  const written = { chunks: 0, facts: 0, notes: 0, secrets: 0, sessions: 0 };
  // A null `seen` bucket means "this export did not cover that subtree", so
  // prune must leave it alone.
  const seen = { chunks: new Set(), facts: new Set(), notes: new Set(), secrets: null, sessions: new Set() };

  try {
    // secrets are opt-in, and every value is validated before anything at
    // all is written so a bad row aborts the whole export.
    let secrets = [];
    if (includeSecrets) {
      secrets = db.all("SELECT key, kind, value, note, updated FROM secrets");
      assertSecretsEncrypted(secrets);
      seen.secrets = new Set();
    }

    // chunks
    let sql = "SELECT id, project, document, metadata, created_at, updated_at FROM chunks";
    const where = [];
    const args = [];
    if (since) { where.push("(updated_at > ? OR created_at > ?)"); args.push(since, since); }
    if (projects && projects.length) {
      where.push(`project IN (${projects.map(() => "?").join(",")})`);
      args.push(...projects);
    }
    if (where.length) sql += " WHERE " + where.join(" AND ");
    for (const c of db.all(sql, ...args)) {
      let meta = {};
      try { meta = JSON.parse(c.metadata || "{}"); } catch {}
      if (!includeSensitive && meta.sensitive) continue;
      const rel = chunkRelPath(c);
      seen.chunks.add(rel);
      if (await writeIfChanged(join(repoDir, rel), chunkToMd(c))) written.chunks++;
    }

    // facts
    for (const f of db.all("SELECT id, subject, predicate, object, timestamp, source_session FROM facts")) {
      const rel = factRelPath(f);
      seen.facts.add(rel);
      if (await writeIfChanged(join(repoDir, rel), factToMd(f))) written.facts++;
    }

    // notes
    for (const n of db.all("SELECT key, value, updated FROM notes")) {
      const rel = noteRelPath(n);
      seen.notes.add(rel);
      if (await writeIfChanged(join(repoDir, rel), noteToMd(n))) written.notes++;
    }

    // secrets (validated ciphertext only, and only when opted in)
    for (const s of secrets) {
      const rel = secretRelPath(s);
      seen.secrets.add(rel);
      if (await writeIfChanged(join(repoDir, rel), secretToMd(s))) written.secrets++;
    }

    // sessions
    for (const s of db.all("SELECT id, project, session_id, summary, created_at FROM session_summaries")) {
      const rel = sessionRelPath(s);
      seen.sessions.add(rel);
      if (await writeIfChanged(join(repoDir, rel), sessionToMd(s))) written.sessions++;
    }
  } finally {
    db.close();
  }

  return { written, seen };
}

export async function pruneRemovedFiles({ repoDir, seen }) {
  // Delete files in repoDir under known subtrees that the export didn't
  // touch this run. Caller decides scope by passing populated `seen`.
  const subtrees = ["chunks", "facts", "notes", "secrets", "sessions"];
  let removed = 0;
  for (const sub of subtrees) {
    const root = join(repoDir, sub);
    if (!existsSync(root)) continue;
    for await (const abs of walk(root)) {
      const rel = relative(repoDir, abs).split(sep).join("/");
      const set = seen[sub];
      if (!set) continue; // subtree not covered by this export — never prune it
      if (!set.has(rel)) {
        await rm(abs);
        removed++;
      }
    }
  }
  return removed;
}

async function* walk(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const p = join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(p);
    else if (entry.isFile()) yield p;
  }
}

// ---- importer (markdown -> memnest HTTP API) ----
// We never write the sqlite directly. The HTTP server is the canonical
// owner; this keeps invariants (FTS index, vector store) intact.

// A core started with MEMNEST_TOKEN rejects every unauthenticated request,
// so mirror that env var here. The value is only ever put in the request
// header: never logged, echoed, or written to a journal file.
function authHeader() {
  const token = (process.env.MEMNEST_TOKEN ?? "").trim();
  return token ? { authorization: `Bearer ${token}` } : {};
}

async function httpJSON(url, method, body) {
  const r = await fetch(url, {
    method,
    headers: { ...(body ? { "content-type": "application/json" } : {}), ...authHeader() },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await r.text();
  if (!r.ok) throw new Error(`HTTP ${r.status} ${method} ${url}: ${text.slice(0, 200)}`);
  try { return JSON.parse(text); } catch { return text; }
}

export async function importChangedFiles({ repoDir, baseURL = "http://127.0.0.1:3111", files }) {
  // Apply user edits back into memnest. Strategy:
  //   chunks/*: POST /add as a new memory with a provenance marker; the
  //             original chunk is left intact (see below).
  //   notes/*:  counted as pending only. The server does expose
  //             POST /notes, but writing note edits back is not
  //             implemented in this release.
  //   secrets/*: refuse — secrets must be set via API, not file edits.
  const stats = { chunks_applied: 0, chunks_skipped: 0, notes_pending: 0, errors: [] };
  for (const f of files) {
    const abs = join(repoDir, f);
    const text = await readFile(abs, "utf8");
    const { data, body } = parseFrontmatter(text);
    try {
      if (f.startsWith("chunks/")) {
        // This deliberately adds rather than replaces, so the pre-edit
        // chunk stays available as audit evidence (POST /update exists if
        // you want an in-place correction). memnest dedupes identical
        // (project, text) pairs, so we append a hidden provenance marker:
        // it makes the post unique even when the body is unchanged, and
        // gives reviewers an audit trail.
        const project = data.project && !/^(root|default|global)$/.test(data.project)
          ? data.project : "playbook";
        const marker = `\n\n<!-- memnest-journal: edited-from=${data.id ?? "unknown"} at=${new Date().toISOString()} -->`;
        await httpJSON(`${baseURL}/add`, "POST", {
          project,
          text: body.replace(/\n+$/, "") + marker,
          metadata: {
            chunk_type: data.chunk_type ?? "manual",
            importance: data.importance ?? "knowledge",
            edited_from: data.id ?? null,
            edited_via: "memnest-journal",
          },
        });
        stats.chunks_applied++;
      } else if (f.startsWith("notes/")) {
        // Reported only; note write-back is not implemented in this release.
        stats.notes_pending++;
      } else if (f.startsWith("secrets/")) {
        stats.chunks_skipped++; // explicit no-op
      }
    } catch (e) {
      stats.errors.push({ file: f, error: String(e.message || e) });
    }
  }
  return stats;
}
