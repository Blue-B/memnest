import assert from "node:assert/strict";
import { eventToRequest, sendEvent } from "./memnest-adapter.mjs";

const add = eventToRequest({
	type: "remember",
	text: "deploy uses 8320",
	project: "demo",
	adapter: "test-host",
	memory_kind: "fact",
});
assert.equal(add.path, "/add");
assert.equal(add.body.metadata.adapter, "test-host");
assert.equal(add.body.metadata.memory_kind, "fact");

const search = eventToRequest({
	type: "search",
	query: "deploy",
	adapter: "test-host",
});
assert.equal(search.path, "/search");
assert.equal(search.body.adapter, "test-host");

const feedback = eventToRequest({
	type: "feedback",
	recall_id: "recall-1",
	outcome: "helpful",
});
assert.equal(feedback.path, "/feedback");
assert.equal(feedback.body.recall_id, "recall-1");
assert.equal(feedback.body.outcome, "helpful");

let captured;
const result = await sendEvent({ type: "health" }, async (url, init) => {
	captured = { url, init };
	return new Response('{"status":"ok"}', { status: 200 });
});
assert.equal(result.status, "ok");
assert.match(captured.url, /\/health$/);

assert.throws(
	() => eventToRequest({ type: "unknown" }),
	/unsupported event type/,
);
console.log("generic-http adapter: 11 assertions passed");
