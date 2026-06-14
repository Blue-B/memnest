/// <reference types="bun-types" />
// Runtime load/registration test: drives the REAL src/index.ts extension entry
// with a fake pi API (no live pi, no model, no engine). Catches registration
// bugs that typechecking can miss — e.g. an invalid tool schema or a throw at
// registration time. The scratchpad tool is also executed end-to-end because
// its working memory is local files (no engine needed).
import { expect, test } from "bun:test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), "memnest-learn-load-"));
process.env.MEMNEST_LEARN_DIR = TMP;
process.env.MEMNEST_URL = "http://127.0.0.1:59999"; // never contacted in this test

const mod = await import("../src/index.js");

function makeFakePi() {
  const hooks: Record<string, Function> = {};
  const tools: any[] = [];
  const api = {
    on: (event: string, handler: Function) => {
      hooks[event] = handler;
    },
    registerTool: (tool: any) => tools.push(tool),
  };
  return { api, hooks, tools };
}

test("extension registers the expected hooks and tools", () => {
  const f = makeFakePi();
  (mod.default as any)(f.api);

  for (const h of [
    "session_start",
    "before_agent_start",
    "agent_end", // assistant-turn capture
    "input",
    "session_before_compact",
    "session_shutdown",
  ]) {
    expect(typeof f.hooks[h]).toBe("function");
  }

  const names = f.tools.map((t) => t.name).sort();
  expect(names).toEqual(["memory_consolidate", "scratchpad", "skill"]);
  for (const t of f.tools) {
    expect(typeof t.label).toBe("string");
    expect(t.label.length).toBeGreaterThan(0);
    expect(typeof t.description).toBe("string");
    expect(typeof t.execute).toBe("function");
    // TypeBox object schema marker (would be undefined for a plain JSON object)
    expect(t.parameters?.type).toBe("object");
  }
});

test("agent_end hook ingests assistant text without an engine or model", async () => {
  const f = makeFakePi();
  (mod.default as any)(f.api);
  // willRetry turns are skipped; a normal turn's assistant text is ingested.
  // No throw == the handler tolerates arbitrary message shapes defensively.
  const agentEnd = f.hooks["agent_end"]!;
  await agentEnd({ willRetry: true, messages: [{ role: "assistant", content: "ignored" }] });
  await agentEnd({
    messages: [
      { role: "assistant", content: [{ type: "text", text: "the bug was a stale lock" }] },
      { role: "tool", content: [{ type: "text", text: "tool noise" }] },
      "garbage",
    ],
  });
});

test("scratchpad tool executes without an engine (working memory is local)", async () => {
  const f = makeFakePi();
  (mod.default as any)(f.api);
  const sp = f.tools.find((t) => t.name === "scratchpad");
  const ctx: any = {
    sessionManager: { getSessionId: () => "sess1234abcd" },
    hasUI: false,
    isIdle: () => true,
  };
  const r1 = await sp.execute("id1", { action: "add", text: "fix the auth bug" }, undefined, undefined, ctx);
  expect(r1.content[0].text).toContain("fix the auth bug");
  expect(r1).toHaveProperty("details"); // required by AgentToolResult
  const r2 = await sp.execute("id2", { action: "list" }, undefined, undefined, ctx);
  expect(r2.content[0].text).toContain("fix the auth bug");
  expect(fs.existsSync(path.join(TMP, "SCRATCHPAD.md"))).toBe(true);
});
