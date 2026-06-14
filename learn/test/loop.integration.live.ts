// Live integration for the closed-loop additions (reinforce / skill-improve /
// user-model) against a REAL throwaway memnest engine over HTTP with
// deterministic stub LLMs. NOT part of `bun test`. Run against a throwaway
// engine (own port + data dir) — never the live store:
//   MEMNEST_URL=http://127.0.0.1:3199 bun run test/loop.integration.live.ts
//
// Distance caps here are deliberately loose vs the production defaults so the
// test asserts the DATA PATH (neighbors -> update/add -> re-index), not the
// exact embedding threshold of one model build.

import { MemnestClient } from "../src/memnest-client.js";
import { reinforce } from "../src/reinforce.js";
import { improveSkills } from "../src/skills.js";
import { updateUserModel, userModelContext } from "../src/user-model.js";
import type { LearnedMemory, LlmComplete } from "../src/types.js";

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
  console.log(`=== memnest-learn CLOSED-LOOP live integration vs ${URL} ===`);

  // ── #1 Outcome reinforcement ────────────────────────────────────────────────
  const P = "ltest";
  await client.add({
    text: "The dev server crashes with EADDRINUSE because port 5173 is already bound by a stale process.",
    project: P,
    category: "failure",
    importance: "knowledge",
  });
  await waitForSearch("dev server EADDRINUSE port already bound", P, (d) =>
    d.some((x) => x.document.includes("EADDRINUSE")),
  );

  const r1 = await reinforce(client, "recurrence", "the dev server still crashes, that port is bound again", {
    project: P,
    maxDistance: 0.6,
  });
  check("reinforce matched the recurring failure", r1.matched && r1.action === "reinforced", JSON.stringify(r1));
  check("reinforce bumped importance to decision", r1.newImportance === "decision", `imp=${r1.newImportance}`);

  await sleep(1500);
  const afterR1 = await waitForSearch("dev server EADDRINUSE port", P, (d) =>
    d.some((x) => x.document.includes("[recurred ×1]")),
  );
  check("recurrence marker persisted on the survivor", afterR1.some((x) => x.document.includes("[recurred ×1]")));

  const r2 = await reinforce(client, "recurrence", "still broken — same EADDRINUSE on that bound port", {
    project: P,
    maxDistance: 0.6,
  });
  check("second recurrence increments the counter", r2.recurred === 2, `recurred=${r2.recurred}`);

  // ── #2 Skill self-improvement ───────────────────────────────────────────────
  const draftLlm: LlmComplete = async ({ user }) =>
    user.startsWith("CURRENT SKILL")
      ? "# Free a bound dev-server port\n1. lsof -i :5173 to find the stale PID\n2. kill it\n3. restart the dev server\n4. if it recurs, add a predev port-cleanup script"
      : "# Free a bound dev-server port\n1. lsof -i :5173 to find the PID\n2. kill the stale process\n3. restart the dev server";

  const seed: LearnedMemory[] = [
    { category: "tool_quirk", text: "When the dev server hits EADDRINUSE on 5173, kill the stale PID holding the port." },
  ];
  const made = await improveSkills(client, seed, draftLlm, { maxDistance: 0.6 });
  check("skill drafted from a procedural learning", made.created === 1, JSON.stringify(made));
  const skillHits = await waitForSearch("free bound dev server port stale PID", "_skills", (d) =>
    d.some((x) => x.document.includes("Free a bound dev-server port")),
  );
  check("drafted skill is searchable in _skills", skillHits.some((x) => x.document.startsWith("# Free a bound")));

  const refine = await improveSkills(
    client,
    [{ category: "insight", text: "Add a predev script that frees the dev-server port before starting." }],
    draftLlm,
    { maxDistance: 0.7 },
  );
  check("existing skill refined (not duplicated)", refine.improved === 1 && refine.created === 0, JSON.stringify(refine));
  await sleep(1200);
  const refined = await client.search("free bound dev server port", { project: "_skills", nResults: 10 });
  check("refined skill carries the new step", refined.some((x) => x.document.includes("port-cleanup script")));

  // ── #4 User model deepening ─────────────────────────────────────────────────
  const umLlm: LlmComplete = async () => "User prefers Bun over npm/pnpm as the package manager for all projects.";
  const facts: LearnedMemory[] = [
    { category: "preference", text: "User wants explanations written in Korean." },
    { category: "preference", text: "User prefers Bun over npm." },
  ];
  const u1 = await updateUserModel(client, facts, umLlm, { maxDistance: 0.6 });
  check("user-model added new facets", u1.added === 2, JSON.stringify(u1));
  // wait until BOTH facets are indexed (HNSW write lag), else the refine
  // neighbour-search below races and the similar facet is added not merged.
  await waitForSearch("_user_model bun npm package manager facet", "_user_model", (d) => d.length >= 2);

  const u2 = await updateUserModel(
    client,
    [{ category: "preference", text: "Actually prefer bun for the package manager everywhere." }],
    umLlm,
    { maxDistance: 0.7 },
  );
  check("similar facet is refined, not duplicated", u2.refined === 1 && u2.added === 0, JSON.stringify(u2));

  const profile = await userModelContext(client, "package manager preference");
  check("userModelContext renders a who-you-are block", profile.startsWith("user_profile:") && profile.includes("Bun"), profile.slice(0, 120));

  console.log(`\n=== ${failures === 0 ? "ALL CLOSED-LOOP CHECKS PASSED" : failures + " CHECK(S) FAILED"} ===`);
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e) => {
  console.error("loop integration crashed:", e);
  process.exit(2);
});
