import { execFileSync } from "node:child_process"
import { readFile, writeFile } from "node:fs/promises"
import { KoreanCodingBenchmark } from "./korean-coding-benchmark"
import { MemnestProvider, type SearchResult } from "./memnest-provider"

interface QueryResult {
  questionId: string
  query: string
  expectedSessionId: string
  durationMs: number
  rank: number | null
  results: SearchResult[]
  error?: string
}

export function percentile(values: number[], fraction: number): number | null {
  if (values.length === 0) return null
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.ceil(fraction * sorted.length) - 1]
}

async function serverStats(baseUrl: string, token?: string): Promise<Record<string, unknown>> {
  const response = await fetch(`${baseUrl}/stats`, {
    headers: token ? { authorization: `Bearer ${token}` } : {},
  })
  if (!response.ok) throw new Error(`Memnest /stats returned ${response.status}`)
  return response.json() as Promise<Record<string, unknown>>
}

async function serverRssBytes(): Promise<number | null> {
  const pid = process.env.MEMNEST_PID
  if (!pid || process.platform !== "linux") return null
  try {
    const status = await readFile(`/proc/${pid}/status`, "utf8")
    const match = status.match(/^VmRSS:\s+(\d+)\s+kB$/m)
    return match ? Number(match[1]) * 1024 : null
  } catch {
    return null
  }
}

async function run(): Promise<void> {
  const outIndex = process.argv.indexOf("--out")
  const outputPath = outIndex >= 0 ? process.argv[outIndex + 1] : undefined
  if (outIndex >= 0 && !outputPath) throw new Error("--out requires a path")

  const baseUrl = (process.env.MEMNEST_URL ?? "http://127.0.0.1:3111").replace(/\/$/, "")
  const token = process.env.MEMNEST_TOKEN?.trim()
  const containerTag = `memorybench-ko-${process.pid}-${Date.now()}`
  const benchmark = new KoreanCodingBenchmark()
  const provider = new MemnestProvider()
  await benchmark.load()
  await provider.initialize({ apiKey: token || "none", baseUrl })

  const questions = benchmark.getQuestions()
  const uniqueSessions = new Map(
    questions.flatMap((question) =>
      benchmark.getHaystackSessions(question.questionId).map((session) => [session.sessionId, session] as const)
    )
  )
  const startedAt = new Date().toISOString()
  const ingestStart = performance.now()
  const ingest = await provider.ingest([...uniqueSessions.values()], { containerTag })
  await provider.awaitIndexing(ingest, containerTag)
  const ingestAndIndexMs = performance.now() - ingestStart

  const queryResults: QueryResult[] = []
  try {
    for (const question of questions) {
      const expectedSessionId = String(question.metadata?.expectedSessionId ?? "")
      const start = performance.now()
      try {
        const results = (await provider.search(question.question, {
          containerTag,
          limit: 10,
        })) as SearchResult[]
        const durationMs = performance.now() - start
        const index = results.findIndex((result) =>
          result.document.includes(`MemoryBench session: ${expectedSessionId}`)
        )
        queryResults.push({
          questionId: question.questionId,
          query: question.question,
          expectedSessionId,
          durationMs,
          rank: index >= 0 ? index + 1 : null,
          results,
        })
      } catch (error) {
        queryResults.push({
          questionId: question.questionId,
          query: question.question,
          expectedSessionId,
          durationMs: performance.now() - start,
          rank: null,
          results: [],
          error: error instanceof Error ? error.message : String(error),
        })
      }
    }

    const durations = queryResults.filter((item) => !item.error).map((item) => item.durationMs)
    const stats = await serverStats(baseUrl, token)
    const disk = stats.disk as Record<string, number> | undefined
    const gitCommit = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim()
    const successful = queryResults.filter((item) => !item.error)
    const report = {
      schemaVersion: 1,
      kind: "retrieval-only",
      startedAt,
      completedAt: new Date().toISOString(),
      environment: {
        memnestCommit: gitCommit,
        memnestUrl: baseUrl,
        embedModel: process.env.MEMNEST_EMBED_MODEL ?? "intfloat/multilingual-e5-base",
        runtime: `Bun ${process.versions.bun ?? "unknown"}`,
        platform: `${process.platform}-${process.arch}`,
        containerTag,
      },
      counts: {
        sessions: uniqueSessions.size,
        questions: questions.length,
        successfulSearches: successful.length,
      },
      metrics: {
        answerAccuracy: null,
        answerAccuracyReason: "Retrieval-only run: no answering or judge model was called.",
        hitAt1: successful.filter((item) => item.rank === 1).length / questions.length,
        recallAt3: successful.filter((item) => item.rank !== null && item.rank <= 3).length / questions.length,
        mrrAt10:
          successful.reduce((sum, item) => sum + (item.rank ? 1 / item.rank : 0), 0) /
          questions.length,
        successRate: successful.length / questions.length,
        searchLatencyMs: {
          p50: percentile(durations, 0.5),
          p95: percentile(durations, 0.95),
        },
        ingestAndIndexMs,
        contextTokens: null,
        contextTokensReason: "MemoryBench computes context tokens during its paid answering phase.",
        serverRssSnapshotBytes: await serverRssBytes(),
        serverRssSnapshotReason: process.env.MEMNEST_PID
          ? "One end-of-run snapshot read from /proc/<MEMNEST_PID>/status when available."
          : "Set MEMNEST_PID to the daemon PID on Linux to capture an end-of-run RSS snapshot.",
        diskBytes: disk
          ? Number(disk.db_bytes ?? 0) +
            Number(disk.text_index_bytes ?? 0) +
            Number(disk.vector_bytes ?? 0)
          : null,
      },
      queries: queryResults,
    }
    const rendered = `${JSON.stringify(report, null, 2)}\n`
    if (outputPath) await writeFile(outputPath, rendered)
    process.stdout.write(rendered)
    if (successful.length !== questions.length) process.exitCode = 1
  } finally {
    await provider.clear(containerTag)
  }
}

if (import.meta.main) await run()
