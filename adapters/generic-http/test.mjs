import assert from "node:assert/strict";
process.env.MEMNEST_TOKEN = "  adapter-token  ";
const { eventToRequest, sendEvent } = await import(
	"./memnest-adapter.mjs?test"
);

const add = eventToRequest({
	type: "remember",
	text: "deploy uses 8320",
	project: "demo",
	adapter: "test-host",
	memory_kind: "fact",
	verified_at: "2026-08-20T00:00:00Z",
});
assert.equal(add.path, "/add");
assert.equal(add.body.metadata.adapter, "test-host");
assert.equal(add.body.metadata.memory_kind, "fact");
assert.equal(add.body.metadata.chunk_type, "manual");
assert.equal(add.body.metadata.verified_at, "2026-08-20T00:00:00Z");

const search = eventToRequest({
	type: "search",
	query: "deploy",
	project: "demo",
	adapter: "test-host",
});
assert.equal(search.path, "/search");
assert.equal(search.body.adapter, "test-host");
const cwdSearch = eventToRequest({
	type: "search",
	query: "deploy",
	cwd: "/work/demo",
});
assert.equal(cwdSearch.body.project, "");
assert.equal(cwdSearch.body.cwd, "/work/demo");

assert.throws(
	() =>
		eventToRequest({
			type: "feedback",
			recall_id: "recall-1",
			outcome: "helpful",
		}),
	/unsupported event type: feedback/,
);

let captured;
const result = await sendEvent({ type: "health" }, async (url, init) => {
	captured = { url, init };
	return new Response('{"status":"ok"}', { status: 200 });
});
assert.equal(result.status, "ok");
assert.match(captured.url, /\/health$/);
assert.equal(captured.init.headers.authorization, "Bearer adapter-token");

assert.throws(
	() => eventToRequest({ type: "search", query: "unsafe" }),
	/search project or cwd is required/,
);
assert.throws(
	() => eventToRequest({ type: "unknown" }),
	/unsupported event type/,
);

// Conversation capture is `memnest watch` only. These two used to be adapter
// operations, so they stay explicitly rejected rather than silently unknown.
for (const type of ["message", "summary"]) {
	assert.throws(
		() => eventToRequest({ type, text: "hello", project: "demo" }),
		/use 'memnest watch'/,
	);
}

console.log("generic-http adapter: 22 assertions passed");
