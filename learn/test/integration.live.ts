// Live integration test: exercises the memnest-learn data path against a REAL
// running memnest engine over HTTP, with a deterministic stub LLM (so the LLM
// content is predictable). NOT part of `bun test` — run explicitly against a
// throwaway engine:
//   MEMNEST_URL=http://127.0.0.1:3199 bun run test/integration.live.ts
//
// It never touches the live palimpsest (different port + data dir).

import { MemnestClient } from "../src/memnest-client.js";
import { captureCorrection, captureMemories } from "../src/capture.js";
import { consolidateByEmbedding } from "../src/consolidate.js";
import type { LlmComplete } from "../src/types.js";

const URL = process.env.MEMNEST_URL ?? "http://127.0.0.1:3199";
const client = new MemnestClient(URL, fetch as any);

let failures = 0;
function check(name: string, cond: boolean, extra = "") {
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${extra ? `  (${extra})` : ""}`);
  if (!cond) failures++;
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function waitForSearch(
  query: string,
  project: string,
  predicate: (docs: { document: string; category: string; id: string }[]) => boolean,
  timeoutMs = 20000,
) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const r = await client.search(query, { project, nResults: 20 });
    if (predicate(r)) return r;
    await sleep(800);
  }
  return client.search(query, { project, nResults: 20 });
}

async function main() {
  console.log(`=== memnest-learn live integration vs ${URL} ===`);

  // --- A. capture: stub LLM extracts categorised memories -> engine ---
  const captureLlm: LlmComplete = async () =>
    JSON.stringify([
      { category: "failure", text: "Storing auth tokens in localStorage is XSS-prone; use httpOnly cookies." },
      { category: "preference", text: "User prefers Bun over npm for this project." },
    ]);
  const cap = await captureMemories(
    [{ role: "user", text: "we hit an auth bug" }],
    captureLlm,
    client,
    { project: "itest" },
  );
  check("capture wrote 2 memories", cap.written.length === 2, `written=${cap.written.length} errors=${cap.errors}`);

  const found = await waitForSearch("localStorage auth token XSS", "itest", (d) =>
    d.some((x) => x.document.includes("XSS-prone")),
  );
  const failChunk = found.find((x) => x.document.includes("XSS-prone"));
  check("captured failure is searchable", !!failChunk);
  check("category round-trips as Failure", failChunk?.category === "Failure", `category=${failChunk?.category}`);

  // --- B. context injection pack includes the memory ---
  const ctx = await client.context("how should we store auth tokens", { project: "itest", maxChars: 4000 });
  check("context pack non-empty + bounded", ctx.prompt.length > 0 && ctx.prompt.length <= 4000, `len=${ctx.prompt.length}`);
  check("context pack contains the failure memory", ctx.prompt.includes("XSS-prone") || ctx.prompt.includes("httpOnly"));

  // --- C. consolidation: two paraphrases -> merge + non-destructive supersede ---
  const P = "itestc";
  await client.add({
    text: "The 03:00 deploy failed because a stuck worker held the database migration lock until we killed it.",
    project: P,
    category: "insight",
  });
  await client.add({
    text: "Our deploy at 03:00 broke since a hung worker was holding the DB migration lock; killing the worker fixed it.",
    project: P,
    category: "insight",
  });
  const cItems = await waitForSearch("deploy failed migration lock stuck worker", P, (d) => d.length >= 2);
  check("two paraphrase chunks persisted (no false dedup)", cItems.length >= 2, `count=${cItems.length}`);

  const mergeLlm: LlmComplete = async () =>
    "Deploy failures from a stuck worker holding the DB migration lock are resolved by killing the worker.";
  // Embedding-based consolidation via the engine's cosine neighbours — catches
  // these paraphrases (which client-side trigram similarity scored ~0.30 and
  // would have missed at the default threshold).
  const res = await consolidateByEmbedding(client, cItems, mergeLlm, { maxDistance: 0.5, apply: true });
  check("consolidate merged one cluster", res.merged >= 1, `clusters=${res.clusters} merged=${res.merged} superseded=${res.superseded}`);

  await sleep(1500); // let the update/supersede re-index
  const afterNormal = await client.search("deploy migration lock", { project: P, nResults: 20 });
  const supersededBucket = await client.search("deploy migration lock", { project: "_superseded", nResults: 20 });
  check("survivor carries merged text", afterNormal.some((x) => x.document.includes("resolved by killing the worker")), `normal=${afterNormal.length}`);
  check("retired duplicate moved to _superseded (non-destructive)", supersededBucket.length >= 1, `superseded=${supersededBucket.length}`);
  check("retired duplicate gone from its project", !afterNormal.some((x) => res.superseded > 0 && x.project === "_superseded"));

  // --- D. correction fast-path ---
  const cid = await captureCorrection("Actually, use pnpm not npm here.", client, "itest");
  check("correction captured", !!cid);
  const corr = await waitForSearch("pnpm npm package manager", "itest", (d) =>
    d.some((x) => x.category === "Correction"),
  );
  check("correction stored with Correction category", corr.some((x) => x.category === "Correction"));

  console.log(`\n=== ${failures === 0 ? "ALL CHECKS PASSED" : failures + " CHECK(S) FAILED"} ===`);
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e) => {
  console.error("integration crashed:", e);
  process.exit(2);
});
