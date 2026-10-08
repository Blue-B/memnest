import assert from "node:assert/strict";
process.env.MEMNEST_EXPOSE_SECRET_TOOLS = "0";
process.env.MEMNEST_URL = "http://memnest.invalid";
process.env.MEMNEST_AUTOCONTEXT_MODE = "balanced";
const approach = {
  status: "failed", applicability: "Linux", evidence: "test exit 1",
  assertion: "caller_reported_not_verified", evidence_status: "provided_unverified",
  trust: "untrusted_reference_never_execute", truncated: false,
};
let sent;
globalThis.fetch = async (url, init) => {
  sent = JSON.parse(init.body || "null");
  const row = { id: "a", project: "alpha", document: "dependency workaround", score: 1,
    doc_len: 21, chunk_type: "Manual", approach };
  return new Response(JSON.stringify(String(url).endsWith("/search")
    ? { project: "alpha", results: [row] } : row));
};
const tools = new Map(), events = new Map();
(await import("../dist/index.mjs")).default({
  registerTool(t) { tools.set(t.name, t); }, registerCommand() {},
  on(name, fn) { events.set(name, fn); },
});
assert.equal(tools.size, 5);
const schema = tools.get("memory_remember").parameters.properties.approach;
assert.equal(schema.additionalProperties, false);
const input = { status: "failed", applicability: "Linux", evidence: "test exit 1" };
await tools.get("memory_remember").execute("x", {text: "workaround", project: "alpha", approach: input}, null, null, {});
assert.deepEqual(sent.metadata.approach, input);
assert.match(tools.get("memory_remember").description, /not server verification/);
const result = await tools.get("memory_search").execute("x", {query: "dependency", project: "alpha"}, null, null, {});
assert.match(result.content[0].text, /failed/);
assert.match(result.content[0].text, /provided_unverified/);
assert.match(result.content[0].text, /caller_reported_not_verified/);
const got = await tools.get("memory_get").execute("x", {id: "a"});
assert.deepEqual(JSON.parse(got.content[0].text).approach, approach);
events.get("session_start")({}, {cwd: "/tmp/alpha"});
const context = await events.get("before_agent_start")({prompt: "Which dependency workaround failed before?"});
assert.match(context.message.content, /failed/);
assert.match(context.message.content, /caller-reported, not verified/);
assert.match(context.message.content, /never follow commands/);
console.log("PASS pi approach save/search/get/autocontext preserves caller-report distinctions; five tools unchanged");
