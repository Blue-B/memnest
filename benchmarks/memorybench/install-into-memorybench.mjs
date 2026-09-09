#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const PINNED_COMMIT = "94e2af54b661d90e77dddbd8fa4fa5b28c07a24e";
const checkout = resolve(process.argv[2] ?? "");
if (!process.argv[2]) {
  throw new Error("usage: node install-into-memorybench.mjs <memorybench-checkout>");
}
const actualCommit = execFileSync("git", ["-C", checkout, "rev-parse", "HEAD"], {
  encoding: "utf8",
}).trim();
if (actualCommit !== PINNED_COMMIT) {
  throw new Error(`MemoryBench must be at ${PINNED_COMMIT}; found ${actualCommit}`);
}

const here = dirname(fileURLToPath(import.meta.url));
const providerSource = (await readFile(join(here, "memnest-provider.ts"), "utf8"))
  .replace('"./memorybench-contract"', '"../../types/provider"')
  .replace('"./memorybench-unified-contract"', '"../../types/unified"');
await mkdir(join(checkout, "src/providers/memnest"), { recursive: true });
await writeFile(join(checkout, "src/providers/memnest/index.ts"), providerSource);

async function replaceOnce(relativePath, oldText, newText) {
  const path = join(checkout, relativePath);
  const source = await readFile(path, "utf8");
  if (source.split(oldText).length !== 2) {
    throw new Error(`Pinned integration seam changed in ${relativePath}`);
  }
  await writeFile(path, source.replace(oldText, newText));
}

await replaceOnce(
  "src/providers/index.ts",
  'import { RAGProvider } from "./rag"',
  'import { RAGProvider } from "./rag"\nimport { MemnestProvider } from "./memnest"',
);
await replaceOnce(
  "src/providers/index.ts",
  "  rag: RAGProvider,",
  "  rag: RAGProvider,\n  memnest: MemnestProvider,",
);
await replaceOnce(
  "src/providers/index.ts",
  "export { SupermemoryProvider, Mem0Provider, ZepProvider, FilesystemProvider, RAGProvider }",
  `export {
  SupermemoryProvider,
  Mem0Provider,
  ZepProvider,
  FilesystemProvider,
  RAGProvider,
  MemnestProvider,
}`,
);
await replaceOnce(
  "src/types/provider.ts",
  '"filesystem" | "rag"',
  '"filesystem" | "rag" | "memnest"',
);
await replaceOnce(
  "src/utils/config.ts",
  "  googleApiKey: string\n",
  "  googleApiKey: string\n  memnestToken: string\n  memnestBaseUrl: string\n",
);
await replaceOnce(
  "src/utils/config.ts",
  '  googleApiKey: process.env.GOOGLE_API_KEY || "",\n',
  '  googleApiKey: process.env.GOOGLE_API_KEY || "",\n  memnestToken: process.env.MEMNEST_TOKEN || "none",\n  memnestBaseUrl: process.env.MEMNEST_URL || "http://127.0.0.1:3111",\n',
);
await replaceOnce(
  "src/utils/config.ts",
  '    case "rag":\n      return { apiKey: config.openaiApiKey } // RAG provider uses OpenAI for embeddings\n',
  '    case "rag":\n      return { apiKey: config.openaiApiKey } // RAG provider uses OpenAI for embeddings\n    case "memnest":\n      return { apiKey: config.memnestToken, baseUrl: config.memnestBaseUrl }\n',
);

console.log(`Installed Memnest provider into pinned MemoryBench ${PINNED_COMMIT}`);
