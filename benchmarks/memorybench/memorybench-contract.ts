// Structural copy of the MemoryBench provider/benchmark interfaces at the
// upstream revision pinned in README.md. Keep this small so interface drift is
// visible without making Memnest depend on MemoryBench internals.
import type { UnifiedSession } from "./memorybench-unified-contract"
export type {
  UnifiedMessage,
  UnifiedQuestion,
  UnifiedSession,
} from "./memorybench-unified-contract"

export interface ProviderConfig {
  apiKey: string
  baseUrl?: string
  [key: string]: unknown
}

export interface IngestOptions {
  containerTag: string
  metadata?: Record<string, unknown>
}

export interface SearchOptions {
  containerTag: string
  limit?: number
  threshold?: number
}

export interface IngestResult {
  documentIds: string[]
  taskIds?: string[]
}

export interface IndexingProgress {
  completedIds: string[]
  failedIds: string[]
  total: number
}

export type IndexingProgressCallback = (progress: IndexingProgress) => void

export interface Provider {
  name: string
  initialize(config: ProviderConfig): Promise<void>
  ingest(sessions: UnifiedSession[], options: IngestOptions): Promise<IngestResult>
  awaitIndexing(
    result: IngestResult,
    containerTag: string,
    onProgress?: IndexingProgressCallback
  ): Promise<void>
  search(query: string, options: SearchOptions): Promise<unknown[]>
  clear(containerTag: string): Promise<void>
}

export interface BenchmarkConfig {
  dataPath?: string
}

export interface QuestionFilter {
  questionTypes?: string[]
  limit?: number
  offset?: number
}

export interface QuestionTypeInfo {
  id: string
  alias: string
  description: string
}

export type QuestionTypeRegistry = Record<string, QuestionTypeInfo>

import type { UnifiedQuestion } from "./memorybench-unified-contract"

export interface Benchmark {
  name: string
  load(config?: BenchmarkConfig): Promise<void>
  getQuestions(filter?: QuestionFilter): UnifiedQuestion[]
  getHaystackSessions(questionId: string): UnifiedSession[]
  getGroundTruth(questionId: string): string
  getQuestionTypes(): QuestionTypeRegistry
}
