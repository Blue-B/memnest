// Background LLM budget — a sliding-window rate limiter for the borrowed host
// model. Capture + skill self-improvement + user-model refinement all fire LLM
// calls in the background; on a busy session that can compete with the user's
// real work for tokens / rate limit. This caps the *automatic* calls (manual
// tools like skill create / memory_consolidate are NOT gated).
//
// Pure + injectable clock so it's unit-testable.

export class LlmBudget {
  private times: number[] = [];

  constructor(
    private readonly maxCalls: number,
    private readonly windowMs: number,
    private readonly now: () => number = Date.now,
  ) {}

  /** Consume one slot if available within the window; false when exhausted. */
  allow(): boolean {
    if (this.maxCalls <= 0) return false;
    const t = this.now();
    const cutoff = t - this.windowMs;
    this.times = this.times.filter((x) => x > cutoff);
    if (this.times.length >= this.maxCalls) return false;
    this.times.push(t);
    return true;
  }

  /** Current usage within the live window (for diagnostics). */
  state(): { used: number; max: number; windowMs: number } {
    const cutoff = this.now() - this.windowMs;
    this.times = this.times.filter((x) => x > cutoff);
    return { used: this.times.length, max: this.maxCalls, windowMs: this.windowMs };
  }
}
