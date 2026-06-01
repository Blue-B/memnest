// Thin git wrapper. We shell out so we work with whatever git the user
// already has configured (SSH keys, GPG signing, credential helper, hooks).

import { spawnSync } from "node:child_process";

function run(args, opts = {}) {
  const r = spawnSync("git", args, { encoding: "utf8", ...opts });
  if (r.status !== 0 && !opts.allowFail) {
    throw new Error(`git ${args.join(" ")} failed: ${r.stderr || r.stdout}`);
  }
  return { stdout: r.stdout || "", stderr: r.stderr || "", status: r.status };
}

export function isRepo(cwd) {
  return run(["rev-parse", "--is-inside-work-tree"], { cwd, allowFail: true }).status === 0;
}

export function init(cwd) {
  run(["init", "-q", "-b", "main"], { cwd });
  // sane defaults for memory repos
  run(["config", "user.email", run(["config","--global","--get","user.email"], { cwd, allowFail: true }).stdout.trim() || "memnest-journal@local"], { cwd });
  run(["config", "user.name",  run(["config","--global","--get","user.name"],  { cwd, allowFail: true }).stdout.trim() || "memnest-journal"], { cwd });
}

export function statusPorcelain(cwd) {
  return run(["status", "--porcelain"], { cwd }).stdout
    .split("\n").filter(Boolean).map(l => ({ code: l.slice(0,2), path: l.slice(3) }));
}

export function changedFilesSinceHead(cwd) {
  // Files modified in working tree since last commit (for import flow).
  const a = run(["diff", "--name-only"], { cwd }).stdout.split("\n").filter(Boolean);
  const b = run(["diff", "--name-only", "--cached"], { cwd }).stdout.split("\n").filter(Boolean);
  return [...new Set([...a, ...b])];
}

export function addAll(cwd) { run(["add", "-A"], { cwd }); }

export function commit(cwd, message, { allowEmpty = false } = {}) {
  const args = ["commit", "-q", "-m", message];
  if (allowEmpty) args.push("--allow-empty");
  const r = run(args, { cwd, allowFail: true });
  if (r.status !== 0 && !/nothing to commit/.test(r.stdout + r.stderr)) {
    throw new Error(`git commit failed: ${r.stderr || r.stdout}`);
  }
  return r.status === 0;
}

export function push(cwd, remote = "origin", branch = "main") {
  run(["push", remote, branch], { cwd });
}

export function log(cwd, n = 10) {
  return run(["log", `-${n}`, "--pretty=format:%h %ad %s", "--date=short"], { cwd, allowFail: true }).stdout;
}
