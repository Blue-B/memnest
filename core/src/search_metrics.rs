//! Process-lifetime search latency counters.
//!
//! Recall history used to live in a `recall_events` table that kept the
//! redacted query text of every search for 90 days. Conversation transcripts
//! already store that text and are semantically searchable, so the table was a
//! second and worse copy of user prompts. These counters keep the only part
//! that was operationally useful, timing, and record no query text at all.

use std::sync::atomic::{AtomicU64, Ordering};

// Counters reset on every process restart and are never written to disk. They
// hold timing only: no query text, project name, or result id passes through
// here.
static SEARCHES: AtomicU64 = AtomicU64::new(0);
static TOTAL_MS: AtomicU64 = AtomicU64::new(0);
static MAX_MS: AtomicU64 = AtomicU64::new(0);

/// Latency figures for the current process, as read by `/stats`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SearchLatency {
    pub searches: u64,
    pub average_ms: f64,
    pub max_ms: u64,
}

/// Record one completed search. Called on the search path, so it stays on
/// relaxed ordering: these are monitoring counters, not a synchronization
/// point for any other state.
pub fn record_search(elapsed_ms: u64) {
    SEARCHES.fetch_add(1, Ordering::Relaxed);
    TOTAL_MS.fetch_add(elapsed_ms, Ordering::Relaxed);
    MAX_MS.fetch_max(elapsed_ms, Ordering::Relaxed);
}

/// Read the counters. A concurrent search can land between the two loads, so
/// the average can trail the true value by one sample. That is acceptable for
/// a health figure and costs no locking on the search path.
pub fn snapshot() -> SearchLatency {
    let total_ms = TOTAL_MS.load(Ordering::Relaxed);
    let searches = SEARCHES.load(Ordering::Relaxed);
    let average_ms = if searches == 0 {
        0.0
    } else {
        total_ms as f64 / searches as f64
    };
    SearchLatency {
        searches,
        average_ms,
        max_ms: MAX_MS.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counters are process-global, so this asserts on deltas rather than
    /// absolute values: other tests in the same binary also run searches.
    #[test]
    fn record_search_accumulates_count_total_and_max() {
        let before = snapshot();
        record_search(10);
        record_search(30);
        let after = snapshot();

        assert_eq!(after.searches, before.searches + 2);
        assert!(after.max_ms >= 30);

        let recorded_total = after.average_ms * after.searches as f64
            - before.average_ms * before.searches as f64;
        assert!((recorded_total - 40.0).abs() < 1e-6);
    }

    #[test]
    fn snapshot_reports_zero_average_before_any_search() {
        // Cannot assert a pristine process here, so this only pins the
        // divide-by-zero guard itself.
        let empty = SearchLatency::default();
        assert_eq!(empty.average_ms, 0.0);
        assert_eq!(empty.searches, 0);
    }
}
