import assert from "node:assert/strict";
process.env.MEMNEST_URL = "http://memnest.invalid";
const requests = [];
let legacy = false;
const legacyText = "😀한".repeat(6000);
globalThis.fetch = async (url, init = {}) => {
  requests.push({ url: String(url), init });
  if (legacy) return new Response(JSON.stringify({ document: legacyText }));
  return new Response(
    JSON.stringify(
      String(url).endsWith("/search")
        ? {
            results: [
              {
                id: "a",
                project: "p",
                score: 1,
                document: "😀한",
                doc_len: 99,
              },
            ],
          }
        : {
            document: "😀한",
            next_offset: 3,
            has_more: true,
            provenance: { session_id: "s" },
            before: [{ id: "b", next_offset: 0 }],
          },
    ),
  );
};
const tools = new Map();
(await import("../dist/index.mjs")).default({
  registerTool(t) {
    tools.set(t.name, t);
  },
  registerCommand() {},
  on() {},
});
assert.equal(
  "max_chars" in tools.get("memory_search").parameters.properties,
  false,
);
const search = await tools
  .get("memory_search")
  .execute("x", { query: "q", project: "p" }, undefined, undefined, {});
assert.equal("max_chars" in JSON.parse(requests.at(-1).init.body), false);
assert.match(search.content[0].text, /😀한/);
assert.doesNotMatch(search.content[0].text, /undefined/);
assert.match(search.content[0].text, /doc_len=99/);
const get = await tools
  .get("memory_get")
  .execute("x", { id: "a/b", offset: 1, max_chars: 2, before: 1, after: 0 });
assert.match(
  requests.at(-1).url,
  /a%2Fb\?offset=1&max_chars=2&before=1&after=0$/,
);
const page = JSON.parse(get.content[0].text);
assert.equal(page.next_offset, 3);
assert.equal(page.provenance.session_id, "s");
assert.equal(page.before[0].next_offset, 0);
legacy = true;
for (const [args, start, size] of [
  [{ id: "a", max_chars: 1 }, 0, 1],
  [{ id: "a", offset: 1, max_chars: 2, before: 0, after: 0 }, 1, 2],
  [{ id: "a" }, 0, 8000],
  [{ id: "a", max_chars: 30000 }, 0, 12000],
  [{ id: "a", offset: 12000 }, 12000, 0],
  [{ id: "a", offset: 13000 }, 13000, 0],
]) {
  const reply = await tools.get("memory_get").execute("x", args);
  const page = JSON.parse(reply.content[0].text);
  assert.equal(
    page.document,
    Array.from(legacyText)
      .slice(start, start + size)
      .join(""),
  );
  assert.equal(page.doc_len, 12000);
  assert.equal(page.returned_chars, size);
  assert.equal(page.total_returned_chars, size);
  assert.equal(page.has_more, start + size < 12000);
  assert.equal(page.next_offset, start + size < 12000 ? start + size : null);
  assert.equal(page.truncated, start > 0 || start + size < 12000);
  assert.equal(page.paging, "client");
}
for (const neighbors of [{ before: 1 }, { after: 1 }]) {
  const unsupported = await tools
    .get("memory_get")
    .execute("x", { id: "a", ...neighbors });
  assert.match(unsupported.content[0].text, /^Error:.*transcript neighbors/);
  assert.doesNotMatch(unsupported.content[0].text, /😀한/);
}
for (const body of [
  "not-json",
  "null",
  "[]",
  '{"document":3}',
  '{"unexpected":"value"}',
]) {
  globalThis.fetch = async () => new Response(body);
  for (const args of [{ id: "a" }, { id: "a", max_chars: 1 }]) {
    const invalid = await tools.get("memory_get").execute("x", args);
    assert.match(invalid.content[0].text, /^Error: Invalid memory response/);
  }
}
console.log(
  "pi bounded retrieval: server paging forwarded; legacy text bounded with Unicode continuation; unsupported neighbors and malformed cores fail closed",
);
