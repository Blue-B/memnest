#!/usr/bin/env node
import { resolve, join } from "node:path";
import { homedir } from "node:os";
import { mkdir, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import {
  exportAll,
  pruneRemovedFiles,
  importChangedFiles,
} from "../src/journal.mjs";
import * as git from "../src/git.mjs";

const HELP = `memnest-journal — your AI memory as a git-backed markdown repo

Usage:
  pjournal init   <dir>           # initialize a journal repo at <dir>
  pjournal export [opts]          # write DB -> markdown (incremental)
  pjournal sync   [opts]          # export, then git add+commit (+push if --push)
  pjournal import [opts]          # apply user edits to *.md back into memnest
  pjournal log    [-n N]          # show commit history of the journal
  pjournal status                 # show pending changes

Common options:
  --dir <path>          Journal repo dir (default: ~/.memnest/journal)
  --db  <path>          memnest sqlite path (default: ~/.memnest/memory.db)
  --url <url>           memnest server URL (default: http://127.0.0.1:3111)
  --project <name,...>  Limit to specific projects (export only)
  --since <iso>         Only chunks newer than <iso> (export only)
  --include-sensitive   Include chunks flagged sensitive=true in export
  --include-secrets     Export the secrets table (validated ciphertext only)
  --prune               Delete files that no longer correspond to DB rows
                        (cannot be combined with --project or --since)
  --push                git push after sync
  --remote <name>       git remote (default: origin)
  --branch <name>       git branch (default: main)
  --message <msg>       custom commit message
`;

function parseArgs(argv) {
  const args = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const k = a.slice(2);
      const next = argv[i + 1];
      if (!next || next.startsWith("--")) args[k] = true;
      else {
        args[k] = next;
        i++;
      }
    } else if (a === "-n") {
      args.n = argv[++i];
    } else args._.push(a);
  }
  return args;
}

function filters(args) {
  const projects = args.project
    ? String(args.project)
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean)
    : null;
  return { projects, since: args.since || null };
}

function assertPrunable({ projects, since }) {
  if (!projects && !since) return;
  throw new Error(
    "--prune cannot be combined with --project or --since: a filtered export only writes part of the " +
      "journal, so pruning would delete (and commit the deletion of) files for memories that still exist. " +
      "Run an unfiltered sync, or use `pjournal export` with the filter and no --prune.",
  );
}

function cfg(args) {
  return {
    dir: resolve(args.dir || join(homedir(), ".memnest", "journal")),
    db: args.db || join(homedir(), ".memnest", "memory.db"),
    url: args.url || "http://127.0.0.1:3111",
  };
}

async function cmdInit(args) {
  const dir = resolve(
    args._[0] || args.dir || join(homedir(), ".memnest", "journal"),
  );
  await mkdir(dir, { recursive: true });
  if (!git.isRepo(dir)) git.init(dir);
  // README + .gitignore (master.key must never be committed)
  const readme = `# memnest-journal\n\nThis repo is the human-readable, version-controlled view of your\nmemnest AI memory. The canonical store is sqlite at \`~/.memnest/\`;\nthis tree is generated from it and can be edited then synced back.\n\n- \`chunks/\` — memory chunks grouped by project\n- \`facts/\`  — structured (subject, predicate, object) facts\n- \`notes/\`  — key-value notes\n- \`secrets/\` — empty unless you pass \`--include-secrets\`. With that flag every\n  row must be AES-256-GCM ciphertext (\`$enc2$...\` or legacy \`$enc$...\`) or\n  the export aborts, so plaintext is never written here. Review before push.\n- \`sessions/\` — session summaries\n\nWorkflow:\n\n  pjournal sync           # export DB -> commit\n  vim chunks/root/foo.md  # edit a memory by hand\n  pjournal import         # apply edits back to memnest\n  pjournal sync --push    # publish to your remote\n`;
  await writeFile(join(dir, "README.md"), readme);
  await writeFile(
    join(dir, ".gitignore"),
    [
      "# never commit these — they belong only in the local memnest store",
      "master.key",
      "memory.db",
      "memory.db-shm",
      "memory.db-wal",
      "*.hnsw",
      "text_index/",
      "vectors/",
      "",
    ].join("\n"),
  );
  await Promise.all(
    ["chunks", "facts", "notes", "secrets", "sessions"].map((sub) =>
      mkdir(join(dir, sub), { recursive: true }),
    ),
  );
  console.log(`initialized journal at ${dir}`);
}

async function cmdExport(args) {
  const c = cfg(args);
  const f = filters(args);
  if (args.prune) assertPrunable(f);
  const { written, seen } = await exportAll({
    dbPath: c.db,
    repoDir: c.dir,
    since: f.since,
    projects: f.projects,
    includeSensitive: !!args["include-sensitive"],
    includeSecrets: !!args["include-secrets"],
  });
  let removed = 0;
  if (args.prune) removed = await pruneRemovedFiles({ repoDir: c.dir, seen });
  console.log(JSON.stringify({ written, removed, dir: c.dir }, null, 2));
}

async function cmdSync(args) {
  const c = cfg(args);
  if (!existsSync(c.dir) || !git.isRepo(c.dir)) {
    throw new Error(`not a journal repo: ${c.dir} (run: pjournal init)`);
  }
  const f = filters(args);
  // sync always prunes, so a filtered sync would commit bogus deletions.
  assertPrunable(f);
  const { written, seen } = await exportAll({
    dbPath: c.db,
    repoDir: c.dir,
    since: f.since,
    projects: f.projects,
    includeSensitive: !!args["include-sensitive"],
    includeSecrets: !!args["include-secrets"],
  });
  const removed = await pruneRemovedFiles({ repoDir: c.dir, seen });
  git.addAll(c.dir);
  const msg =
    args.message ||
    `chore(memory): export ${
      Object.entries(written)
        .filter(([, n]) => n)
        .map(([k, n]) => `${n} ${k}`)
        .join(", ") || "no changes"
    }${removed ? ` (-${removed})` : ""}`;
  const committed = git.commit(c.dir, msg);
  if (args.push)
    git.push(c.dir, args.remote || "origin", args.branch || "main");
  console.log(
    JSON.stringify(
      { committed, written, removed, pushed: !!args.push },
      null,
      2,
    ),
  );
}

async function cmdImport(args) {
  const c = cfg(args);
  if (!git.isRepo(c.dir)) throw new Error(`not a journal repo: ${c.dir}`);
  // Files the user changed since the last commit are candidates.
  const files = git
    .changedFilesSinceHead(c.dir)
    .filter((f) => f.startsWith("chunks/") || f.startsWith("notes/"));
  if (!files.length) {
    console.log(JSON.stringify({ message: "no changes to import" }));
    return;
  }
  const stats = await importChangedFiles({
    repoDir: c.dir,
    baseURL: c.url,
    files,
  });
  console.log(JSON.stringify({ files, ...stats }, null, 2));
}

async function cmdLog(args) {
  const c = cfg(args);
  if (!git.isRepo(c.dir)) throw new Error(`not a journal repo: ${c.dir}`);
  process.stdout.write(git.log(c.dir, Number(args.n) || 20) + "\n");
}

async function cmdStatus(args) {
  const c = cfg(args);
  if (!git.isRepo(c.dir)) throw new Error(`not a journal repo: ${c.dir}`);
  console.log(
    JSON.stringify(
      { dir: c.dir, changes: git.statusPorcelain(c.dir) },
      null,
      2,
    ),
  );
}

async function main() {
  const argv = process.argv.slice(2);
  if (!argv.length || argv[0] === "-h" || argv[0] === "--help") {
    process.stdout.write(HELP);
    return;
  }
  const [cmd, ...rest] = argv;
  const args = parseArgs(rest);
  try {
    switch (cmd) {
      case "init":
        return await cmdInit(args);
      case "export":
        return await cmdExport(args);
      case "sync":
        return await cmdSync(args);
      case "import":
        return await cmdImport(args);
      case "log":
        return await cmdLog(args);
      case "status":
        return await cmdStatus(args);
      default:
        process.stderr.write(`unknown command: ${cmd}\n\n${HELP}`);
        process.exit(2);
    }
  } catch (e) {
    process.stderr.write(`error: ${e.message || e}\n`);
    process.exit(1);
  }
}
main();
