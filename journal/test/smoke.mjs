#!/usr/bin/env node
// End-to-end smoke against a scratch memnest instance.
//
// MEMNEST_URL and MEMNEST_DB have no defaults on purpose. This test stores
// memories, so defaulting to 127.0.0.1:3111 and ~/.memnest/memory.db would
// leave test collections in whatever store the developer actually uses.
//
// Exits 0 if all assertions pass, non-zero otherwise.

import { spawnSync } from "node:child_process";
import { rmSync, existsSync, readFileSync, writeFileSync, readdirSync } from "node:fs";
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

let ok = 0, fail = 0;
function assert(name, cond, detail = "") {
  if (cond) { console.log(`  PASS  ${name}`); ok++; }
  else { console.log(`  FAIL  ${name}${detail ? "  -- " + detail : ""}`); fail++; }
}
function cli(...args) {
  const r = spawnSync(process.execPath, [CLI, ...args, "--dir", DIR, "--db", DB, "--url", URL], { encoding: "utf8" });
  return { code: r.status, out: r.stdout, err: r.stderr };
}

console.log(`smoke: dir=${DIR} url=${URL}`);

// 1. init
let r = cli("init");
assert("init creates repo", r.code === 0 && existsSync(join(DIR, ".git")), r.err);

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
  body: JSON.stringify({ project, text: `this contains the word ${probe} inside a smoke chunk`, metadata: { chunk_type: "manual", importance: "log" } }),
});
const addJson = await addR.json();
assert("HTTP /add returns id", typeof addJson.id === "string", JSON.stringify(addJson));

// Poll search by the unique probe token until the new chunk is findable.
// memnest batches FTS commits; under load this can take 5–10s.
let committed = false;
let lastResults = null;
const deadline = Date.now() + 30000;
while (Date.now() < deadline && !committed) {
  await new Promise(r => setTimeout(r, 500));
  const sr = await fetch(`${URL}/search`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query: probe, n_results: 5 }),
  }).then(r => r.json());
  lastResults = sr.results || [];
  committed = lastResults.some(x => x.id === addJson.id);
}
assert("memnest persisted the new chunk", committed, `probe=${probe} last=${JSON.stringify(lastResults?.map(x=>x.id)).slice(0,200)}`);

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
  assert("import reports 1 file applied", /chunks_applied":\s*1/.test(r.out), r.out);

  let hit = false;
  let sj = null;
  const deadline2 = Date.now() + 30000;
  while (Date.now() < deadline2 && !hit) {
    await new Promise(r => setTimeout(r, 500));
    sj = await fetch(`${URL}/search`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ query: marker, n_results: 3 }),
    }).then(r => r.json());
    hit = (sj.results || []).some(x => x.document.includes(marker));
  }
  assert("edited body shows up in memnest search", hit, JSON.stringify(sj?.results?.[0] || sj).slice(0, 200));
}

// 6. secrets are exported as ciphertext only (never plaintext)
const secretsDir = join(DIR, "secrets");
let secretSafe = true;
if (existsSync(secretsDir)) {
  for (const f of readdirSync(secretsDir)) {
    const t = readFileSync(join(secretsDir, f), "utf8");
    // ciphertext lines start with $enc$ in memnest's encoding
    if (!t.includes("$enc$") && !t.includes("encryption: aes-256-gcm")) {
      secretSafe = false;
      break;
    }
  }
}
assert("secrets exported as ciphertext only", secretSafe);

// 7. --include-sensitive: sensitive chunk is SKIPPED by default,
//    INCLUDED when the flag is set.
const sensitiveProj = `sensitivesmoke${randomBytes(4).toString("hex")}`;
const sensitiveProbe = `sensitiveprobeword${randomBytes(8).toString("hex")}`;
const sadd = await fetch(`${URL}/add`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({
    project: sensitiveProj,
    text: `this is sensitive content ${sensitiveProbe}`,
    metadata: { chunk_type: "manual", importance: "log", sensitive: true },
  }),
}).then(r => r.json());
assert("sensitive /add returns id", typeof sadd.id === "string", JSON.stringify(sadd));

// Wait until indexed.
let sready = false;
const sdeadline = Date.now() + 30000;
while (Date.now() < sdeadline && !sready) {
  await new Promise(r => setTimeout(r, 500));
  const sr = await fetch(`${URL}/search`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query: sensitiveProbe, n_results: 3 }),
  }).then(r => r.json());
  sready = (sr.results || []).some(x => x.id === sadd.id);
}
assert("sensitive chunk persisted", sready);

cli("sync", "--message", "smoke: sensitive");
const sensitivePath = join(DIR, "chunks", sensitiveProj, `${sadd.id}.md`);
assert("sensitive chunk SKIPPED by default export", !existsSync(sensitivePath), sensitivePath);

cli("sync", "--include-sensitive", "--message", "smoke: include sensitive");
assert("sensitive chunk INCLUDED with --include-sensitive", existsSync(sensitivePath), sensitivePath);

// 8. cleanup
rmSync(DIR, { recursive: true, force: true });

console.log(`\nsmoke: ${ok} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
