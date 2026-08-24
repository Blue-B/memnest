#!/usr/bin/env node
// End-to-end smoke against a scratch memnest instance.
//
// MEMNEST_URL and MEMNEST_DB have no defaults on purpose. This test stores
// memories, so defaulting to 127.0.0.1:3111 and ~/.memnest/memory.db would
// leave test collections in whatever store the developer actually uses.
//
// Exits 0 if all assertions pass, non-zero otherwise.

import { spawnSync } from "node:child_process";
import {
  rmSync,
  existsSync,
  readFileSync,
  writeFileSync,
  readdirSync,
  mkdirSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomBytes } from "node:crypto";

const CLI = join(import.meta.dirname, "..", "bin", "cli.mjs");
const URL = process.env.MEMNEST_URL;
const DB = process.env.MEMNEST_DB;
if (!URL || !DB) {
  console.error(
    [
      "smoke: set MEMNEST_URL and MEMNEST_DB to a scratch instance first.",
      "This test stores memories, so it must not run against your own store.",
      "",
      "  memnest --data-dir /tmp/memnest-smoke --port 3150 &",
      "  MEMNEST_URL=http://127.0.0.1:3150 \\",
      "    MEMNEST_DB=/tmp/memnest-smoke/memory.db npm run smoke",
    ].join("\n"),
  );
  process.exit(2);
}
const DIR = join(tmpdir(), `pj-smoke-${Date.now()}`);

let ok = 0,
  fail = 0;
function assert(name, cond, detail = "") {
  if (cond) {
    console.log(`  PASS  ${name}`);
    ok++;
  } else {
    console.log(`  FAIL  ${name}${detail ? "  -- " + detail : ""}`);
    fail++;
  }
}
function cliIn(dir, db, ...args) {
  const r = spawnSync(
    process.execPath,
    [CLI, ...args, "--dir", dir, "--db", db, "--url", URL],
    { encoding: "utf8" },
  );
  return { code: r.status, out: r.stdout, err: r.stderr };
}
function cli(...args) {
  return cliIn(DIR, DB, ...args);
}

// Writable scratch sqlite for the secret-export fixtures. Deliberately not
// reusing src/db.mjs: that module is read-only by design.
async function openWritable(path) {
  if (typeof Bun !== "undefined") {
    const { Database } = await import("bun:sqlite");
    return new Database(path, { create: true });
  }
  try {
    const { DatabaseSync } = await import("node:sqlite");
    return new DatabaseSync(path);
  } catch {
    const mod = await import("better-sqlite3");
    return new (mod.default || mod)(path);
  }
}

// A fixture DB with every table exportAll reads, so a failure can only come
// from the secret row itself and not from a missing table.
async function fixtureDB(name, secretValue) {
  const path = join(tmpdir(), `pj-smoke-${name}-${Date.now()}.db`);
  const db = await openWritable(path);
  db.exec(`
    CREATE TABLE chunks (id TEXT, project TEXT, document TEXT, metadata TEXT, created_at TEXT, updated_at TEXT);
    CREATE TABLE facts (id TEXT, subject TEXT, predicate TEXT, object TEXT, timestamp TEXT, source_session TEXT);
    CREATE TABLE notes (key TEXT, value TEXT, updated TEXT);
    CREATE TABLE secrets (key TEXT, kind TEXT, value TEXT, note TEXT, updated TEXT);
    CREATE TABLE session_summaries (id TEXT, project TEXT, session_id TEXT, summary TEXT, created_at TEXT);
    INSERT INTO secrets VALUES ('fixturekey', 'token', '${secretValue.replace(/'/g, "''")}', '', '2026-01-01T00:00:00Z');
  `);
  db.close();
  return path;
}

console.log(`smoke: dir=${DIR} url=${URL}`);

// 1. init
let r = cli("init");
assert(
  "init creates repo",
  r.code === 0 && existsSync(join(DIR, ".git")),
  r.err,
);

// 2. export+sync writes chunks and commits
r = cli("sync", "--message", "smoke: initial sync");
assert("sync exits 0", r.code === 0, r.err);
assert("chunks/ subtree exists", existsSync(join(DIR, "chunks")));

// 3. add a fresh chunk via HTTP to ensure something new on next sync.
//     memnest /add is queued — we poll /stats until total_chunks
//     increments before re-syncing.
// Probe must be plain alphanumerics with no special chars — memnest's
// BM25 tokenizer drops dash/uppercase tokens for some configurations.
// Probe must be plain alphanumerics with no special chars — memnest's
// BM25 tokenizer drops dash/uppercase tokens for some configurations.
// We also use a fresh project per run because memnest can blacklist
// a project's FTS partition once it has hit certain dedup heuristics.
const probe = `smokeprobeword${randomBytes(8).toString("hex")}`;
const project = `smokeproject${randomBytes(4).toString("hex")}`;
const addR = await fetch(`${URL}/add`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({
    project,
    text: `this contains the word ${probe} inside a smoke chunk`,
    metadata: { chunk_type: "manual", importance: "log" },
  }),
});
const addJson = await addR.json();
assert(
  "HTTP /add returns id",
  typeof addJson.id === "string",
  JSON.stringify(addJson),
);

// Poll search by the unique probe token until the new chunk is findable.
// memnest batches FTS commits; under load this can take 5–10s.
let committed = false;
let lastResults = null;
const deadline = Date.now() + 30000;
while (Date.now() < deadline && !committed) {
  await new Promise((r) => setTimeout(r, 500));
  const sr = await fetch(`${URL}/search`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query: probe, project, n_results: 5 }),
  }).then((r) => r.json());
  lastResults = sr.results || [];
  committed = lastResults.some((x) => x.id === addJson.id);
}
assert(
  "memnest persisted the new chunk",
  committed,
  `probe=${probe} last=${JSON.stringify(lastResults?.map((x) => x.id)).slice(0, 200)}`,
);

// 4. now sync — must surface the new chunk on disk
cli("sync", "--message", "smoke: pick up new chunk");
const newPath = join(DIR, "chunks", project, `${addJson.id}.md`);
assert("new chunk appears in repo", existsSync(newPath), newPath);

// 5. user edit -> import -> search
if (existsSync(newPath)) {
  const txt = readFileSync(newPath, "utf8");
  const [fmRaw, body] = txt.split("\n---\n", 2);
  const fm = fmRaw + "\n---";
  const marker = `smokeeditmarker${randomBytes(8).toString("hex")}`;
  writeFileSync(newPath, fm + "\n" + marker + " " + body.replace(/^\n+/, ""));
  r = cli("import");
  assert("import exits 0", r.code === 0, r.err);
  assert(
    "import reports 1 file applied",
    /chunks_applied":\s*1/.test(r.out),
    r.out,
  );

  let hit = false;
  let sj = null;
  const deadline2 = Date.now() + 30000;
  while (Date.now() < deadline2 && !hit) {
    await new Promise((r) => setTimeout(r, 500));
    sj = await fetch(`${URL}/search`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ query: marker, project, n_results: 3 }),
    }).then((r) => r.json());
    hit = (sj.results || []).some((x) => x.document.includes(marker));
  }
  assert(
    "edited body shows up in memnest search",
    hit,
    JSON.stringify(sj?.results?.[0] || sj).slice(0, 200),
  );
}

// 6. secrets: not exported at all without --include-secrets, and never
//    exported as plaintext even with it.
const secretFiles = (dir) => {
  const d = join(dir, "secrets");
  return existsSync(d) ? readdirSync(d) : [];
};
assert(
  "secrets NOT exported by default",
  secretFiles(DIR).length === 0,
  secretFiles(DIR).join(","),
);

// The `encryption:` frontmatter is generated unconditionally, so it proves
// nothing: the body itself must be supported ciphertext.
const bodyIsCiphertext = (dir, f) => {
  const t = readFileSync(join(dir, "secrets", f), "utf8");
  const body = t.split("\n---\n").slice(1).join("\n---\n");
  return /^\$enc2?\$/.test(body.trim());
};

// 6a. plaintext row -> whole export must abort, writing nothing.
const plainDB = await fixtureDB("plain", "hunter2-plaintext-password");
const plainDir = join(tmpdir(), `pj-smoke-plain-${Date.now()}`);
mkdirSync(plainDir, { recursive: true });
r = cliIn(plainDir, plainDB, "export", "--include-secrets");
assert(
  "plaintext secret aborts export",
  r.code !== 0,
  `code=${r.code} out=${r.out}`,
);
assert(
  "plaintext secret never written to disk",
  secretFiles(plainDir).length === 0,
  secretFiles(plainDir).join(","),
);
assert(
  "abort message does not leak the value",
  !`${r.out}${r.err}`.includes("hunter2"),
  r.err,
);

// 6b. same DB without the opt-in flag: clean export, still no secrets.
r = cliIn(plainDir, plainDB, "export");
assert(
  "export without --include-secrets ignores secrets table",
  r.code === 0 && secretFiles(plainDir).length === 0,
  r.err || secretFiles(plainDir).join(","),
);

// 6c. real ciphertext shape -> exported, and the body is ciphertext.
const encValue = `$enc2$${randomBytes(48).toString("base64")}`;
const encDB = await fixtureDB("enc", encValue);
const encDir = join(tmpdir(), `pj-smoke-enc-${Date.now()}`);
mkdirSync(encDir, { recursive: true });
r = cliIn(encDir, encDB, "export", "--include-secrets");
assert(
  "ciphertext secret exports with --include-secrets",
  r.code === 0 && secretFiles(encDir).length === 1,
  r.err || secretFiles(encDir).join(","),
);
assert(
  "exported secret body is $enc2$ ciphertext",
  secretFiles(encDir).every((f) => bodyIsCiphertext(encDir, f)),
);

const legacyDB = await fixtureDB(
  "legacy-enc",
  `$enc$${randomBytes(48).toString("base64")}`,
);
const legacyDir = join(tmpdir(), `pj-smoke-legacy-enc-${Date.now()}`);
mkdirSync(legacyDir, { recursive: true });
r = cliIn(legacyDir, legacyDB, "export", "--include-secrets");
assert(
  "legacy $enc$ ciphertext remains exportable",
  r.code === 0 &&
    secretFiles(legacyDir).every((f) => bodyIsCiphertext(legacyDir, f)),
  r.err,
);

// 6d. truncated ciphertext (shorter than nonce+tag) is rejected too.
const shortDB = await fixtureDB(
  "short",
  `$enc$${randomBytes(8).toString("base64")}`,
);
const shortDir = join(tmpdir(), `pj-smoke-short-${Date.now()}`);
mkdirSync(shortDir, { recursive: true });
r = cliIn(shortDir, shortDB, "export", "--include-secrets");
assert(
  "truncated $enc$ payload aborts export",
  r.code !== 0 && secretFiles(shortDir).length === 0,
  `code=${r.code}`,
);

for (const p of [
  plainDB,
  encDB,
  legacyDB,
  shortDB,
  plainDir,
  encDir,
  legacyDir,
  shortDir,
])
  rmSync(p, { recursive: true, force: true });

// 6e. filtered sync/export must refuse to prune instead of deleting files
//     the filter excluded.
const chunkCount = () =>
  readdirSync(join(DIR, "chunks"), { recursive: true }).length;
const before = chunkCount();
r = cli("sync", "--project", "definitelynotaproject");
assert(
  "filtered sync refused",
  r.code !== 0 && /--prune cannot be combined/.test(r.err),
  r.err,
);
r = cli("sync", "--since", "2099-01-01T00:00:00Z");
assert("since-filtered sync refused", r.code !== 0, r.err);
r = cli("export", "--prune", "--project", "definitelynotaproject");
assert(
  "filtered export --prune refused",
  r.code !== 0 && /--prune cannot be combined/.test(r.err),
  r.err,
);
assert(
  "refused runs deleted nothing",
  chunkCount() === before,
  `${before} -> ${chunkCount()}`,
);
r = cli("export", "--project", "definitelynotaproject");
assert("filtered export without --prune still allowed", r.code === 0, r.err);

// 7. --include-sensitive: sensitive chunk is SKIPPED by default,
//    INCLUDED when the flag is set.
//
// The row is written straight into a fixture DB rather than posted to /add:
// the core rejects sensitive=true on the public write path (those values
// belong in the vault), so HTTP can no longer produce this fixture. The
// exporter reads sqlite directly, which is exactly what is under test here.
const sensitiveProj = `sensitivesmoke${randomBytes(4).toString("hex")}`;
const sensitiveId = `manual_${randomBytes(8).toString("hex")}`;
const sensitiveDB = await fixtureDB("sensitive", "$enc$unused");
{
  const db = await openWritable(sensitiveDB);
  db.exec(
    `INSERT INTO chunks VALUES ('${sensitiveId}', '${sensitiveProj}', 'this is sensitive content', ` +
      `'{"chunk_type":"manual","importance":"log","sensitive":true}', ` +
      `'2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');`,
  );
  db.close();
}
const sensitiveDir = join(tmpdir(), `pj-smoke-sensitive-${Date.now()}`);
r = cliIn(sensitiveDir, sensitiveDB, "init");
assert("sensitive fixture repo init", r.code === 0, r.err);

const sensitivePath = join(
  sensitiveDir,
  "chunks",
  sensitiveProj,
  `${sensitiveId}.md`,
);
r = cliIn(sensitiveDir, sensitiveDB, "export");
assert("sensitive export exits 0", r.code === 0, r.err);
assert(
  "sensitive chunk SKIPPED by default export",
  !existsSync(sensitivePath),
  sensitivePath,
);

r = cliIn(sensitiveDir, sensitiveDB, "export", "--include-sensitive");
assert("sensitive export --include-sensitive exits 0", r.code === 0, r.err);
assert(
  "sensitive chunk INCLUDED with --include-sensitive",
  existsSync(sensitivePath),
  sensitivePath,
);
rmSync(sensitiveDir, { recursive: true, force: true });
rmSync(sensitiveDB, { force: true });

// 8. cleanup
rmSync(DIR, { recursive: true, force: true });

console.log(`\nsmoke: ${ok} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
