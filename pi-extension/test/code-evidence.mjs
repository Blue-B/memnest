import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

process.env.MEMNEST_BIN ??= new URL("../../core/target/debug/memnest", import.meta.url).pathname;
process.env.MEMNEST_EXPOSE_SECRET_TOOLS = "0";
delete process.env.MEMNEST_ALLOWED_MODEL_PROVIDERS;
const root = await mkdtemp(join(tmpdir(), "memnest-pi-code-"));
let saved;
let calls = 0;
try {
  await writeFile(join(root, "code.rs"), "first version\n");
  globalThis.fetch = async (url, init) => {
    calls++;
    if (String(url).endsWith("/add")) {
      saved = JSON.parse(init.body);
      return new Response(JSON.stringify({status:"succeeded", id:"test-id"}));
    }
    return new Response(JSON.stringify({id:"test-id", project:"alpha", document:"repair report", next_offset:null,
      code_evidence:saved.metadata.code_evidence}));
  };
  const tools = new Map();
  (await import("../dist/index.mjs")).default({registerTool(t){tools.set(t.name,t);},registerCommand(){},on(){}});
  assert.equal(tools.size, 5);
  const input = {text:"repair report", project:"alpha", code_files:["code.rs"]};
  const result = await tools.get("memory_remember").execute("x", input, null, null, {cwd:root});
  assert.match(result.content[0].text, /succeeded/);
  assert.equal(saved.metadata.code_evidence.files[0].path, "code.rs");
  assert.match(saved.metadata.code_evidence.files[0].sha256, /^[0-9a-f]{64}$/);
  assert.ok(!JSON.stringify(saved).includes("first version"));
  const get = tools.get("memory_get");
  let row = await get.execute("x", {id:"test-id",check_code:true}, null, null, {cwd:root});
  assert.equal(JSON.parse(row.content[0].text).code_check.status, "unchanged");
  await writeFile(join(root, "code.rs"), "changed version\n");
  row = await get.execute("x", {id:"test-id",check_code:true}, null, null, {cwd:root});
  assert.equal(JSON.parse(row.content[0].text).code_check.status, "changed");
  const before = calls;
  const bad = await tools.get("memory_remember").execute("x", {...input,code_files:["../outside"]}, null, null, {cwd:root});
  assert.match(bad.content[0].text, /Cannot capture/);
  assert.equal(calls, before);
  console.log("PASS pi capture/compare runs the real core CLI; source contents not stored; path traversal refused; five tools unchanged");
} finally {
  await rm(root, {recursive:true, force:true});
}
