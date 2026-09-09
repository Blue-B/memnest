import { describe, expect, test } from "bun:test"
import { MemnestProvider, formatSession } from "../memnest-provider"

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  })
}

describe("MemnestProvider MemoryBench contract", () => {
  test("maps initialize, ingest, indexing, search, and clear to the local API", async () => {
    const requests: Array<{ url: string; init: RequestInit }> = []
    const fakeFetch = async (input: string | URL | Request, init: RequestInit = {}) => {
      const url = String(input)
      requests.push({ url, init })
      if (url.endsWith("/health")) return jsonResponse({ status: "ok" })
      if (url.endsWith("/add")) return jsonResponse({ status: "succeeded", id: "manual-1" }, 201)
      if (url.endsWith("/search")) {
        return jsonResponse({
          results: [
            { id: "manual-1", document: "LRU 5분", score: 0.8 },
            { id: "manual-2", document: "distractor", score: 0.2 },
          ],
          project: "question-run",
          total: 2,
          elapsed_ms: 4,
        })
      }
      if (url.endsWith("/prune")) return jsonResponse({ matched: 1, deleted: 1, ids: ["manual-1"] })
      return jsonResponse({ error: "unexpected" }, 500)
    }

    const provider = new MemnestProvider()
    await provider.initialize({
      apiKey: "local-token",
      baseUrl: "http://localhost:3111/",
      fetchImpl: fakeFetch as typeof fetch,
    })
    const ingested = await provider.ingest(
      [
        {
          sessionId: "세션-1",
          metadata: { date: "2026-01-10" },
          messages: [{ role: "user", content: "LRU 캐시는 5분 유지합니다." }],
        },
      ],
      { containerTag: "question-run" }
    )
    expect(ingested).toEqual({ documentIds: ["manual-1"] })

    let progress: unknown
    await provider.awaitIndexing(ingested, "question-run", (value) => (progress = value))
    expect(progress).toEqual({
      completedIds: ["manual-1"],
      failedIds: [],
      total: 1,
    })

    const results = await provider.search("캐시", {
      containerTag: "question-run",
      limit: 5,
      threshold: 0.5,
    })
    expect(results).toHaveLength(2)
    await provider.clear("question-run")

    expect(requests.map((request) => new URL(request.url).pathname)).toEqual([
      "/health",
      "/add",
      "/search",
      "/prune",
    ])
    for (const request of requests) {
      expect(new Headers(request.init.headers).get("authorization")).toBe("Bearer local-token")
    }
    expect(JSON.parse(String(requests[1].init.body))).toMatchObject({
      project: "question-run",
      metadata: {
        chunk_type: "manual",
        session_id: "세션-1",
        adapter: "memorybench",
      },
    })
    expect(JSON.parse(String(requests[2].init.body))).toEqual({
      query: "캐시",
      project: "question-run",
      n_results: 5,
      adapter: "memorybench",
    })
    expect(JSON.parse(String(requests[3].init.body))).toEqual({
      project: "question-run",
      keep_latest: 0,
    })
  })

  test("rejects a malformed search envelope", async () => {
    const provider = new MemnestProvider()
    await provider.initialize({
      apiKey: "none",
      fetchImpl: (async (input) =>
        String(input).endsWith("/health")
          ? jsonResponse({ status: "ok" })
          : jsonResponse([])) as typeof fetch,
    })
    await expect(
      provider.search("query", { containerTag: "run" })
    ).rejects.toThrow("missing its results array")
  })

  test("supports an unauthenticated local daemon and reports HTTP failures", async () => {
    let authorization: string | null = "unexpected"
    const provider = new MemnestProvider()
    await provider.initialize({
      apiKey: "none",
      fetchImpl: (async (_input, init) => {
        authorization = new Headers(init?.headers).get("authorization")
        return jsonResponse({ status: "ok" })
      }) as typeof fetch,
    })
    expect(authorization).toBeNull()

    const failing = new MemnestProvider()
    await expect(
      failing.initialize({
        apiKey: "none",
        fetchImpl: (async () => new Response("down", { status: 503 })) as typeof fetch,
      })
    ).rejects.toThrow("Memnest /health returned 503: down")
  })

  test("formats session metadata and Korean messages without lossy JSON escaping", () => {
    expect(
      formatSession({
        sessionId: "ko-1",
        metadata: { formattedDate: "2026년 1월 10일" },
        messages: [
          {
            role: "user",
            speaker: "민수",
            timestamp: "10:00",
            content: "포트는 8320입니다.",
          },
        ],
      })
    ).toBe("MemoryBench session: ko-1\nDate: 2026년 1월 10일\n\n[민수 (10:00)] 포트는 8320입니다.")
  })
})
