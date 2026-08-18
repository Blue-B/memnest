// REAL-MODEL quality harness. Unlike loop.integration.live.ts (stub LLM, asserts
// the data path), this drives the ACTUAL extension prompts through a real model
// so we can judge the *quality* of the LLM-driven steps: extraction, skill
// draft/refine, user-model refinement.
//
//   bun run test/quality.live.ts        (needs throwaway engine on $MEMNEST_URL)
//
// The model is reached via `pi -p -nt` (tools OFF, so it can never write to any
// live store) — the real complete() bridge the extension uses at runtime.

import { MemnestClient } from "../src/memnest-client.js";
import { captureMemories } from "../src/capture.js";
import { improveSkills } from "../src/skills.js";
import { updateUserModel } from "../src/user-model.js";
import type { LlmComplete, TranscriptTurn } from "../src/types.js";

const URL = process.env.MEMNEST_URL ?? "http://127.0.0.1:3199";
// Clean OpenAI-compatible endpoint (tokenrouter) = the faithful equivalent of
// the extension's pi-ai complete() path: ONLY the system/user we pass, none of
// the `pi -p` CLI's AGENTS.md/persona/memory-format injection (which otherwise
// makes the model answer in the agent's own memory schema instead of ours).
const MODEL = process.env.QUALITY_MODEL ?? "gemini-2.5-flash";
const GKEY = process.env.GEMINI_API_KEY ?? "";
const client = new MemnestClient(URL, fetch as any);

let calls = 0;
const realLlm: LlmComplete = async ({ system, user }) => {
  calls++;
  const res = await fetch(
    `https://generativelanguage.googleapis.com/v1beta/models/${MODEL}:generateContent?key=${GKEY}`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        systemInstruction: { parts: [{ text: system }] },
        contents: [{ role: "user", parts: [{ text: user }] }],
        generationConfig: { temperature: 0 },
      }),
    },
  );
  const j: any = await res.json();
  if (!res.ok) throw new Error(`gemini ${res.status}: ${JSON.stringify(j).slice(0, 200)}`);
  return String(j?.candidates?.[0]?.content?.parts?.[0]?.text ?? "").trim();
};

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const hr = (t: string) => console.log(`\n${"─".repeat(70)}\n${t}\n${"─".repeat(70)}`);

async function main() {
  console.log(`=== REAL-MODEL quality harness  model=${MODEL}  engine=${URL} ===`);

  const transcript: TranscriptTurn[] = [
    { role: "user", text: "the dev deploy keeps dying right after start" },
    { role: "assistant", text: "That's EADDRINUSE on port 5173 — a stale Vite dev server is still holding the port. Find the PID with `lsof -i :5173` and kill it before restarting." },
    { role: "user", text: "ok that worked. also from now on always use bun in this repo, never npm — the npm lockfile keeps fighting CI" },
    { role: "assistant", text: "Got it, switching to bun. Note: the esbuild-based bundler here needs `--force` on its very first run in a clean checkout or it panics on a missing cache dir." },
    { role: "user", text: "good. and please always explain things to me in Korean from now on" },
    { role: "assistant", text: "알겠습니다. 앞으로 한국어로 설명드리겠습니다." },
  ];

  // ── 1. EXTRACTION quality ────────────────────────────────────────────────
  hr("1) CAPTURE (real extraction) — judge: durable? right category? no chatter?");
  const cap = await captureMemories(transcript, realLlm, client, { project: "qtest", max: 10 });
  console.log(`extracted ${cap.memories.length}, written ${cap.written.length}, errors ${cap.errors}`);
  for (const m of cap.memories) console.log(`  [${m.category}] ${m.text}`);
  await sleep(2500); // let the engine index

  // ── 2. SKILL self-improvement quality ────────────────────────────────────
  hr("2) SKILL draft/refine (real) — judge: is the drafted skill actually usable?");
  const s1 = await improveSkills(client, cap.memories, realLlm, { max: 3 });
  console.log(`improveSkills: created=${s1.created} improved=${s1.improved}`);
  await sleep(1500);
  const skills = await client.search("dev server port bun bundler", { project: "_skills", nResults: 10 });
  for (const s of skills) console.log(`  • ${s.document.replace(/\n/g, "\\n").slice(0, 220)}`);

  // refine pass: feed a related new learning, expect it to sharpen an existing skill
  const s2 = await improveSkills(
    client,
    [{ category: "insight", text: "Add a predev npm/bun script that frees port 5173 automatically before the dev server starts." }],
    realLlm,
    { max: 1 },
  );
  console.log(`refine pass: created=${s2.created} improved=${s2.improved}`);

  // ── 3. USER-MODEL quality ────────────────────────────────────────────────
  hr("3) USER MODEL (real) — judge: are facets sharp + correctly merged?");
  const u1 = await updateUserModel(client, cap.memories, realLlm, { max: 4 });
  console.log(`updateUserModel: added=${u1.added} refined=${u1.refined}`);
  await sleep(1500);
  const facets = await client.search("user preferences package manager language", { project: "_user_model", nResults: 10 });
  for (const f of facets) console.log(`  • ${f.document.slice(0, 180)}`);

  console.log(`\n=== done. real model calls: ${calls} ===`);
}

main().catch((e) => {
  console.error("quality harness crashed:", e?.message ?? e);
  process.exit(1);
});
