import type {
  IndexingProgressCallback,
  IngestOptions,
  IngestResult,
  Provider,
  ProviderConfig,
  SearchOptions,
} from "./memorybench-contract"
import type { UnifiedSession } from "./memorybench-unified-contract"

type Fetch = typeof fetch

interface MemnestConfig extends ProviderConfig {
  fetchImpl?: Fetch
}

interface AddResponse {
  id?: string
  status?: string
  error?: string
}

interface SearchResult {
  id: string
  document: string
  score: number
  [key: string]: unknown
}

/** MemoryBench provider backed by Memnest's local HTTP API. */
export class MemnestProvider implements Provider {
  name = "memnest"
  concurrency = { default: 20, ingest: 10, indexing: 20 }

  private baseUrl = "http://127.0.0.1:3111"
  private token: string | undefined
  private fetchImpl: Fetch = fetch

  async initialize(config: MemnestConfig): Promise<void> {
    this.baseUrl = (config.baseUrl || "http://127.0.0.1:3111").replace(/\/$/, "")
    this.token = config.apiKey && config.apiKey !== "none" ? config.apiKey : undefined
    this.fetchImpl = config.fetchImpl ?? fetch
    await this.request("/health", { method: "GET" })
  }

  async ingest(sessions: UnifiedSession[], options: IngestOptions): Promise<IngestResult> {
    const documentIds: string[] = []
    for (const session of sessions) {
      const response = await this.request<AddResponse>("/add", {
        method: "POST",
        body: JSON.stringify({
          project: options.containerTag,
          text: formatSession(session),
          metadata: {
            chunk_type: "manual",
            importance: "knowledge",
            memory_kind: "record",
            session_id: session.sessionId,
            source: "memorybench",
            adapter: "memorybench",
            adapter_version: "1",
          },
        }),
      })
      if (!response.id || response.status === "failed" || response.status === "error") {
        throw new Error(
          `Memnest did not ingest ${session.sessionId}: ${response.error ?? response.status ?? "missing id"}`
        )
      }
      documentIds.push(response.id)
    }
    return { documentIds }
  }

  async awaitIndexing(
    result: IngestResult,
    _containerTag: string,
    onProgress?: IndexingProgressCallback
  ): Promise<void> {
    // POST /add returns only after embedding and both indexes have been persisted.
    onProgress?.({
      completedIds: [...result.documentIds],
      failedIds: [],
      total: result.documentIds.length,
    })
  }

  async search(query: string, options: SearchOptions): Promise<unknown[]> {
    const results = await this.request<SearchResult[]>("/search", {
      method: "POST",
      body: JSON.stringify({
        query,
        project: options.containerTag,
        n_results: options.limit ?? 30,
        adapter: "memorybench",
      }),
    })
    if (!Array.isArray(results)) throw new Error("Memnest /search returned a non-array response")
    // MemoryBench thresholds are provider-specific. Memnest hybrid scores are
    // not normalized probabilities, so applying (for example) its default 0.3
    // client-side would incorrectly discard valid RRF results.
    return results
  }

  async clear(containerTag: string): Promise<void> {
    await this.request("/prune", {
      method: "POST",
      body: JSON.stringify({ project: containerTag, keep_latest: 0 }),
    })
  }

  private async request<T = unknown>(path: string, init: RequestInit): Promise<T> {
    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      ...init,
      headers: {
        "content-type": "application/json",
        ...(this.token ? { authorization: `Bearer ${this.token}` } : {}),
        ...init.headers,
      },
    })
    const text = await response.text()
    if (!response.ok) throw new Error(`Memnest ${path} returned ${response.status}: ${text}`)
    try {
      return JSON.parse(text) as T
    } catch {
      throw new Error(`Memnest ${path} returned invalid JSON`)
    }
  }
}

export function formatSession(session: UnifiedSession): string {
  const date = session.metadata?.formattedDate ?? session.metadata?.date
  const header = [`MemoryBench session: ${session.sessionId}`]
  if (typeof date === "string" && date) header.push(`Date: ${date}`)
  const messages = session.messages.map((message) => {
    const speaker = message.speaker || message.role
    const timestamp = message.timestamp ? ` (${message.timestamp})` : ""
    return `[${speaker}${timestamp}] ${message.content}`
  })
  return [...header, "", ...messages].join("\n")
}

export default MemnestProvider
