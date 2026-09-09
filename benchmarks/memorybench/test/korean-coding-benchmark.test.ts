import { describe, expect, test } from "bun:test"
import { KoreanCodingBenchmark, validateFixture } from "../korean-coding-benchmark"

describe("KoreanCodingBenchmark", () => {
  test("loads the pinned fixture and exposes deterministic MemoryBench views", async () => {
    const benchmark = new KoreanCodingBenchmark()
    await benchmark.load()

    expect(benchmark.getQuestions().map((question) => question.questionId)).toEqual([
      "ko-q-cache",
      "ko-q-test",
      "ko-q-port",
    ])
    expect(benchmark.getQuestions({ questionTypes: ["coding-update"] })).toHaveLength(1)
    expect(benchmark.getQuestions({ offset: 1, limit: 1 })[0].questionId).toBe("ko-q-test")
    expect(benchmark.getGroundTruth("ko-q-port")).toContain("8320")
    expect(benchmark.getHaystackSessions("ko-q-cache")).toHaveLength(3)
    expect(benchmark.getQuestionTypes()["coding-fact"].alias).toBe("fact")
  })

  test("rejects fixture references that cannot be validated", () => {
    expect(() =>
      validateFixture({
        sessions: [{ sessionId: "only", messages: [{ role: "user", content: "증거" }] }],
        questions: [
          {
            questionId: "bad",
            question: "질문",
            questionType: "coding-fact",
            groundTruth: "답",
            haystackSessionIds: ["missing"],
            metadata: { expectedSessionId: "only", requiredTerms: ["답"] },
          },
        ],
      })
    ).toThrow("references missing")
  })
})
