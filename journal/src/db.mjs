// Read-only access to memnest sqlite. Uses better-sqlite3 if available
// (Node), else falls back to bun:sqlite when running under Bun. Keeps a
// tiny abstraction so the CLI works under both runtimes without a binary
// build step on install (better-sqlite3 needs node-gyp on some OSes).

import { existsSync } from "node:fs";

export async function openDB(dbPath, { readonly = true } = {}) {
  if (!existsSync(dbPath)) {
    throw new Error(`memnest db not found at ${dbPath}`);
  }
  // Prefer bun:sqlite (zero-build) when present
  if (typeof Bun !== "undefined") {
    const { Database } = await import("bun:sqlite");
    const db = new Database(dbPath, { readonly });
    return wrapBun(db);
  }
  // Try node:sqlite (Node 22.5+ built-in, no native build needed).
  // Requires --experimental-sqlite on 22.5–23; stable from 24.
  try {
    const { DatabaseSync } = await import("node:sqlite");
    const db = new DatabaseSync(dbPath, { readonly });
    return wrapNodeSqlite(db);
  } catch (e1) {
    // Last resort: better-sqlite3 (needs native build on install).
    try {
      const mod = await import("better-sqlite3");
      const Database = mod.default || mod;
      const db = new Database(dbPath, { readonly });
      return wrapBetter(db);
    } catch (e2) {
      throw new Error(
        "memnest-journal: no SQLite driver available. Install one of:\n" +
        "  - Run under Bun (\u2265 1.0): bun:sqlite is built in.\n" +
        "  - Run under Node 24+ (or Node 22.5\u201323 with --experimental-sqlite).\n" +
        "  - Or: npm i better-sqlite3 (native build, needs python+toolchain).\n" +
        `Underlying errors:\n  node:sqlite \u2014 ${e1?.message ?? e1}\n  better-sqlite3 \u2014 ${e2?.message ?? e2}`,
      );
    }
  }
}

function wrapBun(db) {
  return {
    all(sql, ...args) { return db.query(sql).all(...args); },
    get(sql, ...args) { return db.query(sql).get(...args); },
    close() { db.close(); },
  };
}
function wrapNodeSqlite(db) {
  return {
    all(sql, ...args) { return db.prepare(sql).all(...args); },
    get(sql, ...args) { return db.prepare(sql).get(...args); },
    close() { db.close(); },
  };
}
function wrapBetter(db) {
  return {
    all(sql, ...args) { return db.prepare(sql).all(...args); },
    get(sql, ...args) { return db.prepare(sql).get(...args); },
    close() { db.close(); },
  };
}
