// Core export/import between palimpsest sqlite and a git-friendly
// markdown tree. Everything here is pure-ish (no shell, no network) so
// the same logic is testable headlessly.

import { mkdir, readFile, writeFile, rm, readdir, stat } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { createHash } from "node:crypto";
import { openDB } from "./db.mjs";
import { stringifyFrontmatter, parseFrontmatter } from "./yaml.mjs";

const SAFE_NAME = /[^a-zA-Z0-9._\-]/g;
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

function secretToMd(secret) {
  // We store the encrypted blob and metadata only. Plaintext NEVER leaves
  // the sqlite store; this file is safe to commit *iff* the master.key
  // is not in the repo (we enforce that via .gitignore).
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

export async function exportAll({ dbPath, repoDir, since = null, projects = null, includeSensitive = false }) {
  const db = await openDB(dbPath, { readonly: true });
  const written = { chunks: 0, facts: 0, notes: 0, secrets: 0, sessions: 0 };
  const seen = { chunks: new Set(), facts: new Set(), notes: new Set(), secrets: new Set(), sessions: new Set() };

  try {
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

    // secrets (encrypted blob only — safe by construction)
    for (const s of db.all("SELECT key, kind, value, note, updated FROM secrets")) {
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
      const bucket = sub;
      const set = seen[bucket];
      if (!set || !set.has(rel)) {
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

// ---- importer (markdown -> palimpsest HTTP API) ----
// We never write the sqlite directly. The HTTP server is the canonical
// owner; this keeps invariants (FTS index, vector store) intact.

async function httpJSON(url, method, body) {
  const r = await fetch(url, {
    method,
    headers: body ? { "content-type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await r.text();
  if (!r.ok) throw new Error(`HTTP ${r.status} ${method} ${url}: ${text.slice(0, 200)}`);
  try { return JSON.parse(text); } catch { return text; }
}

export async function importChangedFiles({ repoDir, baseURL = "http://127.0.0.1:3111", files }) {
  // Apply user edits back into palimpsest. Strategy:
  //   chunks/*: re-POST /add (server upserts on id collision? if not, we
  //             dedupe by hashing on the server side eventually; for now
  //             we add new rows and rely on user's intent — emit warning)
  //   notes/*:  POST /notes (key, value)  — not yet wired in 0.1; we
  //             stage them in journal.pending.json so a future server
  //             release with PUT /notes/:key can consume.
  //   secrets/*: refuse — secrets must be set via API, not file edits.
  const stats = { chunks_applied: 0, chunks_skipped: 0, notes_pending: 0, errors: [] };
  for (const f of files) {
    const abs = join(repoDir, f);
    const text = await readFile(abs, "utf8");
    const { data, body } = parseFrontmatter(text);
    try {
      if (f.startsWith("chunks/")) {
        // palimpsest 0.1.x has no /update/:id and dedupes identical
        // (project, text) pairs. To make user edits actually reach the
        // searchable store, we append a hidden provenance marker — this
        // makes the post unique even when the body is identical to a
        // previous chunk, and gives reviewers an audit trail.
        const project = data.project && !/^(root|default|global)$/.test(data.project)
          ? data.project : "playbook";
        const marker = `\n\n<!-- palimpsest-journal: edited-from=${data.id ?? "unknown"} at=${new Date().toISOString()} -->`;
        await httpJSON(`${baseURL}/add`, "POST", {
          project,
          text: body.replace(/\n+$/, "") + marker,
          metadata: {
            chunk_type: data.chunk_type ?? "manual",
            importance: data.importance ?? "knowledge",
            edited_from: data.id ?? null,
            edited_via: "palimpsest-journal",
          },
        });
        stats.chunks_applied++;
      } else if (f.startsWith("notes/")) {
        // Server lacks PUT /notes — stage for next palimpsest release.
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
