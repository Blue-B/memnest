#!/usr/bin/env node
// Trigger tests for pi-memnest autocontext risk rules and tokenizer.
//
// Verifies:
//   1. Korean prompts still fire the risk lanes they fired before.
//   2. English prompts fire the equivalent lanes (memory/absence used to be
//      Korean-only, so non-Korean users had autocontext effectively off).
//   3. Neutral prompts stay quiet, so injection is not unconditional.
//   4. topicTokens keeps non-Latin and accented scripts.
//
// The real src/autocontext.ts is bundled on the fly rather than copied, so a
// regex edit that breaks behaviour fails here instead of passing against a
// stale duplicate.
//
// Run: node test/autocontext-triggers.mjs

import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const ESBUILD = join(ROOT, "node_modules", ".bin", "esbuild");

let ok = 0,
	fail = 0;
function assert(name, cond, msg = "") {
	if (cond) {
		ok++;
		console.log(`  PASS  ${name}`);
	} else {
		fail++;
		console.log(`  FAIL  ${name}  -- ${msg}`);
	}
}

const tmp = mkdtempSync(join(tmpdir(), "memnest-autocontext-"));
const outfile = join(tmp, "autocontext.mjs");
let mod;
try {
	execFileSync(
		ESBUILD,
		[
			join(ROOT, "src", "autocontext.ts"),
			"--bundle",
			"--format=esm",
			"--platform=node",
			"--target=node20",
			`--outfile=${outfile}`,
		],
		{ stdio: ["ignore", "ignore", "inherit"] },
	);
	mod = await import(pathToFileURL(outfile).href);
} catch (e) {
	console.error(`failed to bundle src/autocontext.ts: ${e.message}`);
	rmSync(tmp, { recursive: true, force: true });
	process.exit(2);
}

const { riskLabels, topicTokens, isSubstantive } = mod;
for (const [name, fn] of Object.entries({
	riskLabels,
	topicTokens,
	isSubstantive,
})) {
	if (typeof fn !== "function") {
		console.error(`autocontext.ts does not export ${name}`);
		rmSync(tmp, { recursive: true, force: true });
		process.exit(2);
	}
}

const has = (prompt, label) => riskLabels(prompt).includes(label);

// --- Korean prompts (must keep working) -------------------------------------
assert(
	"ko memory: 전에 말했던 그 설정 기억나?",
	has("전에 말했던 그 설정 기억나?", "memory"),
	riskLabels("전에 말했던 그 설정 기억나?").join(","),
);
assert(
	"ko absence: 그 기능은 지원 안 되는거야?",
	has("그 기능은 지원 안 되는거야?", "absence"),
	riskLabels("그 기능은 지원 안 되는거야?").join(","),
);
assert(
	"ko credential: 깃허브 계정 로그인 토큰 어디 저장했지",
	has("깃허브 계정 로그인 토큰 어디 저장했지", "credential"),
	riskLabels("깃허브 계정 로그인 토큰 어디 저장했지").join(","),
);

// --- English prompts (the regression this test exists for) ------------------
assert(
	"en memory: do you remember what we decided last time?",
	has("do you remember what we decided last time?", "memory"),
	riskLabels("do you remember what we decided last time?").join(","),
);
assert(
	"en memory: you forgot the constraint I gave you earlier",
	has("you forgot the constraint I gave you earlier", "memory"),
	riskLabels("you forgot the constraint I gave you earlier").join(","),
);
assert(
	"en absence: this doesn't work, the flag is not supported",
	has("this doesn't work, the flag is not supported", "absence"),
	riskLabels("this doesn't work, the flag is not supported").join(","),
);
assert(
	"en absence: I can't build it, the binary is missing",
	has("I can't build it, the binary is missing", "absence"),
	riskLabels("I can't build it, the binary is missing").join(","),
);
assert(
	"en credential: rotate the api_key and the oauth refresh token",
	has("rotate the api_key and the oauth refresh token", "credential"),
	riskLabels("rotate the api_key and the oauth refresh token").join(","),
);
assert(
	"en money: what is our pricing and monthly revenue plan",
	has("what is our pricing and monthly revenue plan", "money"),
	riskLabels("what is our pricing and monthly revenue plan").join(","),
);
assert(
	"en config: set up the environment variables and default threshold",
	has("set up the environment variables and default threshold", "config"),
	riskLabels("set up the environment variables and default threshold").join(","),
);

// --- Must NOT trigger -------------------------------------------------------
assert(
	"neutral: rename this variable to userCount please",
	riskLabels("rename this variable to userCount please").length === 0,
	riskLabels("rename this variable to userCount please").join(","),
);
assert(
	"neutral: add a null check to the parse helper in utils",
	riskLabels("add a null check to the parse helper in utils").length === 0,
	riskLabels("add a null check to the parse helper in utils").join(","),
);

// --- Word boundaries actually bound -----------------------------------------
assert(
	"boundary: 'airplane' does not fire credential via 'plan'",
	!has("draw an airplane in ascii art for the header", "credential"),
	riskLabels("draw an airplane in ascii art for the header").join(","),
);
assert(
	"boundary: 'loads' does not fire money via 'ads'",
	!has("the page loads all rows into one giant array", "money"),
	riskLabels("the page loads all rows into one giant array").join(","),
);
assert(
	"boundary: 're' prefix kept, 'reconfigure the proxy' still fires config",
	has("reconfigure the proxy in front of the service", "config"),
	riskLabels("reconfigure the proxy in front of the service").join(","),
);

// --- Tokenizer keeps non-ASCII scripts --------------------------------------
assert(
	"tokens: accented latin survives (café)",
	topicTokens("le café est ouvert").has("café"),
	[...topicTokens("le café est ouvert")].join(","),
);
assert(
	"tokens: japanese survives",
	topicTokens("設定ファイルを更新して").size > 0,
	"empty token set",
);
assert(
	"tokens: cyrillic survives",
	topicTokens("обнови конфигурацию сервера").has("сервера"),
	[...topicTokens("обнови конфигурацию сервера")].join(","),
);
assert(
	"tokens: ascii + digits still tokenised",
	topicTokens("bump memnest to v2 build").has("memnest"),
	[...topicTokens("bump memnest to v2 build")].join(","),
);

// --- isSubstantive gate -----------------------------------------------------
assert(
	"substantive: slash command is skipped",
	!isSubstantive("/memnest status and recent search latency"),
	"slash command passed the gate",
);
assert(
	"substantive: long real prompt passes",
	isSubstantive("please refactor the retry loop in the http client"),
	"real prompt was rejected",
);

rmSync(tmp, { recursive: true, force: true });
console.log(`\nautocontext-triggers: ${ok} passed, ${fail} failed`);
process.exit(fail === 0 ? 0 : 1);
