import { readFile } from "node:fs/promises"
import { fileURLToPath } from "node:url"
import type {
  Benchmark,
  BenchmarkConfig,
  QuestionFilter,
  QuestionTypeRegistry,
  UnifiedQuestion,
  UnifiedSession,
} from "./memorybench-contract"

interface Fixture {
  sessions: UnifiedSession[]
  questions: UnifiedQuestion[]
}

const TYPES: QuestionTypeRegistry = {
  "coding-fact": {
    id: "coding-fact",
    alias: "fact",
    description: "한국어로 기록된 프로젝트별 구현 사실 회상",
  },
  "coding-update": {
    id: "coding-update",
    alias: "update",
    description: "이전 결정을 대체한 최신 코딩 결정 회상",
  },
}

/** Small deterministic fixture adapter for Korean coding-memory smoke tests. */
export class KoreanCodingBenchmark implements Benchmark {
  name = "korean-coding"
  private questions: UnifiedQuestion[] = []
  private sessions = new Map<string, UnifiedSession>()

  async load(config: BenchmarkConfig = {}): Promise<void> {
    const defaultPath = fileURLToPath(
      new URL("./fixtures/korean-coding-memory.json", import.meta.url)
    )
    const fixture = JSON.parse(await readFile(config.dataPath ?? defaultPath, "utf8")) as Fixture
    validateFixture(fixture)
    this.questions = fixture.questions
    this.sessions = new Map(fixture.sessions.map((session) => [session.sessionId, session]))
  }

  getQuestions(filter: QuestionFilter = {}): UnifiedQuestion[] {
    let questions = [...this.questions]
    if (filter.questionTypes?.length) {
      const selected = new Set(filter.questionTypes)
      questions = questions.filter((question) => selected.has(question.questionType))
    }
    const offset = filter.offset ?? 0
    return questions.slice(offset, filter.limit === undefined ? undefined : offset + filter.limit)
  }

  getHaystackSessions(questionId: string): UnifiedSession[] {
    const question = this.findQuestion(questionId)
    return question.haystackSessionIds.map((id) => this.sessions.get(id)!)
  }

  getGroundTruth(questionId: string): string {
    return this.findQuestion(questionId).groundTruth
  }

  getQuestionTypes(): QuestionTypeRegistry {
    return TYPES
  }

  private findQuestion(questionId: string): UnifiedQuestion {
    const question = this.questions.find((candidate) => candidate.questionId === questionId)
    if (!question) throw new Error(`Unknown Korean coding-memory question: ${questionId}`)
    return question
  }
}

export function validateFixture(fixture: Fixture): void {
  if (!Array.isArray(fixture.sessions) || !Array.isArray(fixture.questions)) {
    throw new Error("Fixture must contain sessions and questions arrays")
  }
  const sessionIds = new Set<string>()
  for (const session of fixture.sessions) {
    if (!session.sessionId || sessionIds.has(session.sessionId) || !session.messages?.length) {
      throw new Error(`Invalid or duplicate session: ${session.sessionId || "missing id"}`)
    }
    sessionIds.add(session.sessionId)
  }
  const questionIds = new Set<string>()
  for (const question of fixture.questions) {
    if (!question.questionId || questionIds.has(question.questionId)) {
      throw new Error(`Invalid or duplicate question: ${question.questionId || "missing id"}`)
    }
    questionIds.add(question.questionId)
    if (!TYPES[question.questionType])
      throw new Error(`Unknown question type: ${question.questionType}`)
    if (!question.groundTruth.trim())
      throw new Error(`Missing ground truth: ${question.questionId}`)
    for (const id of question.haystackSessionIds) {
      if (!sessionIds.has(id)) throw new Error(`Question ${question.questionId} references ${id}`)
    }
    const expectedSessionId = question.metadata?.expectedSessionId
    const requiredTerms = question.metadata?.requiredTerms
    if (
      typeof expectedSessionId !== "string" ||
      !sessionIds.has(expectedSessionId) ||
      !question.haystackSessionIds.includes(expectedSessionId)
    ) {
      throw new Error(`Question ${question.questionId} has invalid expectedSessionId`)
    }
    if (
      !Array.isArray(requiredTerms) ||
      requiredTerms.length === 0 ||
      !requiredTerms.every((term) => typeof term === "string" && term.length > 0)
    ) {
      throw new Error(`Question ${question.questionId} has invalid requiredTerms`)
    }
    const expectedText = fixture.sessions
      .find((session) => session.sessionId === expectedSessionId)!
      .messages.map((message) => message.content)
      .join("\n")
    if (
      !requiredTerms.every(
        (term) => expectedText.includes(term) && question.groundTruth.includes(term)
      )
    ) {
      throw new Error(
        `Question ${question.questionId} terms do not match evidence and ground truth`
      )
    }
  }
}

export default KoreanCodingBenchmark
