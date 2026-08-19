// KV-cache-stable memory snapshot.
//
// Local prefix-caching runtimes (llama.cpp, vLLM, MLX) invalidate the cache from
// the first divergent token onward. If the memory block injected into the system
// prompt changes every turn, the entire conversation tail is reprocessed each
// turn. To keep the prefix byte-stable we snapshot the memory context at
// deliberate checkpoints and emit the *same bytes* for every turn in between.
//
// (Idea adopted from jayzeng/pi-memory; generalised here so the builder can be
// any async function — e.g. memnest /context + local working memory.)

export type SnapshotReason =
  | "session_start"
  | "before_compact"
  | "day_rollover"
  | "first_turn";

export interface SnapshotState {
  text: string | null;
  takenAt: string | null;
  takenOnDate: string | null;
  reason: SnapshotReason | null;
}

export function emptySnapshot(): SnapshotState {
  return { text: null, takenAt: null, takenOnDate: null, reason: null };
}

export interface Clock {
  isoNow(): string;
  today(): string; // YYYY-MM-DD
}

export const systemClock: Clock = {
  isoNow: () => new Date().toISOString(),
  today: () => new Date().toISOString().slice(0, 10),
};

/**
 * Holds the byte-stable snapshot and decides when it must be rebuilt.
 *
 * `refresh` is the only place the (possibly expensive, cache-busting) builder
 * runs. `get` returns the cached bytes and refreshes lazily only when a
 * checkpoint condition is met: never-built, or the captured day no longer
 * matches today.
 *
 * There is deliberately no "a memory was written, rebuild now" path. Capture
 * writes land on nearly every turn, and rebuilding on each one would change the
 * snapshot header and invalidate the whole prompt prefix cache, which costs far
 * more than showing a new memory one session late. New memories surface at the
 * next session_start, compaction, or day rollover.
 */
export class MemorySnapshot {
  private state: SnapshotState = emptySnapshot();

  constructor(private readonly clock: Clock = systemClock) {}

  /** Force a rebuild at an intentional checkpoint. */
  async refresh(reason: SnapshotReason, builder: () => Promise<string>): Promise<void> {
    const text = await builder();
    this.state = {
      text,
      takenAt: this.clock.isoNow(),
      takenOnDate: this.clock.today(),
      reason,
    };
  }

  /** True when `get` would rebuild rather than serve the cached bytes. */
  needsRefresh(): boolean {
    return this.state.text === null || this.state.takenOnDate !== this.clock.today();
  }

  private nextReason(): SnapshotReason {
    return this.state.text === null ? "first_turn" : "day_rollover";
  }

  /**
   * Return the byte-stable memory block, rebuilding only at checkpoints. The
   * returned text is identical across turns until a checkpoint fires, keeping
   * the prefix cache warm.
   */
  async get(builder: () => Promise<string>): Promise<{ text: string; reason: SnapshotReason | null; takenAt: string | null }> {
    if (this.needsRefresh()) {
      await this.refresh(this.nextReason(), builder);
    }
    return { text: this.state.text ?? "", reason: this.state.reason, takenAt: this.state.takenAt };
  }

  peek(): SnapshotState {
    return { ...this.state };
  }
}
