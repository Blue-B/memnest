#!/usr/bin/env node
// E2E test: simulate a Claude Desktop-style stdio MCP client launching
// `palimpsest --mcp`, perform initialize -> tools/list -> a real call.
//
// This is the same protocol Claude Desktop, Cursor, Cline, Continue, and
// Zed use. If this passes, registering palimpsest in any of them will work.
//
// Run: node test/e2e-mcp.mjs
//
// IMPORTANT: stop any long-running palimpsest HTTP server first —
// SQLite write lock contention will hang memory_search responses.
//   systemctl --user stop palimpsest    (run the test)    systemctl --user start palimpsest

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join } from "node:path";

const BIN = process.env.PALIMPSEST_BIN ?? "palimpsest";
// Use shared ~/.palimpsest so the embedding model is already warm. Fall
// back to a fresh temp dir when the test wants full isolation (slow).
const DATA = process.env.PALIMPSEST_DATA_DIR ?? join(homedir(), ".palimpsest");
const FRESH = process.env.PALIMPSEST_FRESH === "1";
const dataDir = FRESH ? mkdtempSync(join(tmpdir(), "pp-e2e-")) : DATA;

let ok = 0, fail = 0;
function assert(name, cond, msg = "") {
  if (cond) { ok++; console.log(`  PASS  ${name}`); }
  else { fail++; console.log(`  FAIL  ${name}  -- ${msg}`); }
}

const child = spawn(BIN, ["--mcp", "--data-dir", dataDir], { stdio: ["pipe", "pipe", "pipe"] });

let buf = "";
const inbox = [];
let pendingResolve = null;
child.stdout.on("data", (chunk) => {
  buf += chunk.toString("utf8");
  let nl;
  while ((nl = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, nl).trim();
    buf = buf.slice(nl + 1);
    if (!line) continue;
    try {
      const j = JSON.parse(line);
      if (pendingResolve) { const r = pendingResolve; pendingResolve = null; r(j); }
      else inbox.push(j);
    } catch {}
  }
});

const stderr = [];
child.stderr.on("data", (d) => stderr.push(d.toString("utf8")));

function send(obj) {
  child.stdin.write(JSON.stringify(obj) + "\n");
}

function recv(timeoutMs = 5000) {
  return new Promise((resolve, reject) => {
    if (inbox.length) return resolve(inbox.shift());
    pendingResolve = resolve;
    setTimeout(() => {
      if (pendingResolve) {
        pendingResolve = null;
        reject(new Error("timeout waiting for stdout JSON-RPC line"));
      }
    }, timeoutMs);
  });
}

try {
  // 1. initialize
  send({
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: {
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo: { name: "pi-palimpsest-e2e", version: "0.0.0" },
    },
  });
  const init = await recv(8000);
  assert("initialize returns serverInfo", init?.result?.serverInfo?.name === "palimpsest",
         JSON.stringify(init).slice(0, 200));

  send({ jsonrpc: "2.0", method: "notifications/initialized" });

  // 2. tools/list
  send({ jsonrpc: "2.0", id: 2, method: "tools/list" });
  const list = await recv(8000);
  const names = (list?.result?.tools ?? []).map((t) => t.name);
  assert("tools/list returns >= 12 tools", names.length >= 12, `got ${names.length}: ${names.join(",")}`);
  for (const want of ["memory_add", "memory_search", "memory_stats", "secret_set", "secret_get", "secret_list"]) {
    assert(`tool '${want}' present`, names.includes(want));
  }

  // 3. tools/call memory_stats
  send({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "memory_stats", arguments: {} } });
  const stats = await recv(8000);
  const statsText = stats?.result?.content?.[0]?.text ?? "";
  assert("memory_stats returns total_chunks JSON", /total_chunks/.test(statsText), statsText.slice(0, 200));

  // 4. tools/call memory_add then memory_search
  send({
    jsonrpc: "2.0",
    id: 4,
    method: "tools/call",
    params: {
      name: "memory_add",
      arguments: {
        project: "e2eproject",
        text: "e2eprobeword smoke check from pi-palimpsest e2e harness",
      },
    },
  });
  const add = await recv(8000);
  assert("memory_add returns content", !!add?.result?.content?.[0]?.text, JSON.stringify(add).slice(0, 200));

  // Poll search via tools/call. Fresh data-dir means indexing is fast.
  let found = false;
  for (let i = 0; i < 20 && !found; i++) {
    await new Promise((r) => setTimeout(r, 500));
    const id = 100 + i;
    send({
      jsonrpc: "2.0",
      id,
      method: "tools/call",
      params: { name: "memory_search", arguments: { query: "e2eprobeword", n_results: 3 } },
    });
    let s;
    try {
      s = await recv(3000);
    } catch {
      continue;
    }
    const t = s?.result?.content?.[0]?.text ?? "";
    if (/e2eprobeword/.test(t)) found = true;
  }
  assert("memory_search finds the added chunk via stdio MCP", found);
} catch (e) {
  console.error("e2e error:", e.message);
  fail++;
} finally {
  child.kill("SIGTERM");
  if (FRESH) rmSync(dataDir, { recursive: true, force: true });
}

if (stderr.length) {
  const s = stderr.join("");
  if (s.trim()) console.log("\n(stderr from palimpsest):\n" + s.split("\n").slice(0, 8).join("\n"));
}

console.log(`\ne2e: ${ok} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
