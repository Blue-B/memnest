#!/usr/bin/env node
// Automatic recall against a real core: the built extension asks POST /context,
// so the core's on-topic gate decides what a prompt gets. Starts its own core on
// a temporary data directory and never talks to an installed service.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const bin = process.env.MEMNEST_BIN ?? "memnest";
const data = mkdtempSync(join(tmpdir(), "memnest-autocontext-e2e-"));
const cwd = mkdtempSync(join(tmpdir(), "memnest-autocontext-ws-"));
const modelCache =
	process.env.MEMNEST_MODEL_CACHE ?? join(here, "..", "..", "core", "target", "test-model-cache");
if (existsSync(modelCache)) symlinkSync(modelCache, join(data, "models"), "dir");

const port =
	Number(process.env.MEMNEST_E2E_PORT) ||
	(await new Promise((resolve) => {
		const probe = createServer().listen(0, "127.0.0.1", () => {
			const { port } = probe.address();
			probe.close(() => resolve(port));
		});
	}));
const url = `http://127.0.0.1:${port}`;
const { MEMNEST_TOKEN: _token, ...childEnv } = process.env;
const core = spawn(bin, ["--host", "127.0.0.1", "--port", String(port), "--data-dir", data], {
	stdio: ["ignore", "ignore", "pipe"],
	env: childEnv,
});
const stderr = [];
core.stderr.on("data", (chunk) => stderr.push(chunk.toString()));

const post = async (path, body) => {
	const res = await fetch(`${url}${path}`, {
		method: "POST",
		headers: { "content-type": "application/json" },
		body: JSON.stringify(body),
	});
	assert.ok(res.ok, `${path} -> ${res.status} ${await res.clone().text()}`);
	return res.json();
};

const MEMORIES = {
	port: "스테이징 API 서버의 배포 포트는 8320이고 운영은 프록시 뒤 443을 쓴다.",
	backups: "Database backups run nightly at 03:00 and are kept for 14 days on the NAS.",
	model: "pi 기본 모델은 settings.json의 defaultModel 값 deepseek-flash로 정했다.",
	bios: "BIOS was updated to version F15 with Q-Flash; the XMP memory profile is enabled and fast boot is off.",
	payments: "결제 모듈은 토스페이먼츠를 쓰고 정기결제는 아직 붙이지 않았다.",
	coffee: "The user prefers dark roast coffee and dislikes meetings before 10am.",
	release: "릴리스는 금요일 오후에 하지 않는다. 화요일 오전에 태그를 만들고 배포한다.",
	lint: "Run cargo clippy with -D warnings before every commit; CI rejects warnings.",
	fonts: "보고서 글꼴은 본문 Pretendard 11pt, 제목은 굵게 14pt로 통일한다.",
	cache: "ok so now we then restart the cache server again",
	restart: "지금 그럼 일단 캐시 서버부터 다시 켜보자 어떻게 되는지",
	vpn: "The office VPN drops idle sessions after 30 minutes; reconnect with the tray icon.",
};
// [prompt, the memory the card must show, or null for no card at all]
const PROMPTS = [
	["그럼 이제 어떻게 해? 다시 해줘 일단", null],
	["ok so what should we do now then", null],
	["그러면 그렇게 해줘 일단 지금 다시 계속", null],
	["스테이징 배포 포트가 어떻게 되지", "port"],
	["pi 기본 모델 defaultModel 값은 뭐로 정했지", "model"],
	["how long do we keep the database backups", "backups"],
	["write a short poem about autumn leaves falling", null],
];

try {
	for (let tries = 0; ; tries++) {
		try {
			if ((await fetch(`${url}/health`)).ok) break;
		} catch {}
		assert.ok(tries < 600 && core.exitCode === null, `core did not start\n${stderr.join("").slice(-2000)}`);
		await new Promise((resolve) => setTimeout(resolve, 200));
	}
	const ids = {};
	for (const [name, text] of Object.entries(MEMORIES))
		ids[(await post("/add", { text, cwd, metadata: { chunk_type: "manual", importance: "knowledge", adapter: "e2e" } })).id] = name;
	assert.equal(Object.keys(ids).length, 12, "a dozen distinct saved memories");

	delete process.env.MEMNEST_TOKEN;
	delete process.env.MEMNEST_AUTOCONTEXT_DISABLE;
	process.env.MEMNEST_URL = url;
	process.env.MEMNEST_AUTOCONTEXT_MODE = "balanced";
	const hooks = new Map();
	const extension = await import(`${join(here, "..", "dist", "index.mjs")}?e2e-autocontext`);
	extension.default({
		registerTool() {},
		registerCommand() {},
		on(name, fn) {
			if (!hooks.has(name)) hooks.set(name, fn);
		},
	});

	const named = (text) =>
		Object.entries(MEMORIES)
			.filter(([, memory]) => text.includes(memory.slice(0, 24).replaceAll("&", "&amp;")))
			.map(([name]) => name);
	const rows = [];
	for (const [prompt, expected] of PROMPTS) {
		// A new session each time: the per-session card limit is not what is tested here.
		hooks.get("session_start")({}, { cwd });
		const card = (await hooks.get("before_agent_start")({ prompt }))?.message?.content ?? "";
		const gate = named(card);
		// What the path before the gate would have put in the card: /search, score >= 0.25, top 2.
		const search = await post("/search", { query: prompt, project: "", cwd, n_results: 20, durable_only: true });
		const scoreGate = search.results.filter((r) => r.score >= 0.25).slice(0, 2).map((r) => ids[r.id]);
		rows.push({ prompt, gate: gate.join(",") || "-", score_threshold: scoreGate.join(",") || "-" });
		if (expected === null) assert.equal(card, "", `nothing for: ${prompt} (got ${gate})`);
		else {
			assert.ok(gate.includes(expected), `${prompt}: expected ${expected}, got ${gate}`);
			assert.match(card, /\(on-topic-gate\)/);
		}
	}
	console.table(rows);
	console.log("e2e autocontext: 3 vague and 1 unrelated prompt inject nothing; Korean, mixed and English prompts inject their memory");
} finally {
	core.kill();
	await new Promise((resolve) => (core.exitCode === null ? core.once("exit", resolve) : resolve()));
	rmSync(data, { recursive: true, force: true });
	rmSync(cwd, { recursive: true, force: true });
}
