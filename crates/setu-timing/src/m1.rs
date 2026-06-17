//! M1 finalized-throughput metrics (counters + distributions).
//!
//! Where M0 (stage.rs/histogram.rs) measures **per-event stage latency** to find the
//! submit-side bottleneck, M1 measures **finalized throughput** and the consensus-side
//! constraints that bound it (M0-report §11b-3 / docs/feat/m1-finalized-throughput/):
//!
//! - finalized TPS (events applied/sec) vs submit TPS — the gap is the finalized loss.
//! - CF size (events folded per ConsensusFrame) + fold interval — fold cadence (C2).
//! - finalize latency (event ingress -> applied) — end-to-end terminal latency.
//! - cold-parent rejects + depth_diff distribution — the max-200 limit (C3).
//! - OCC conflicts (old_value-mismatch skips) — hot-object contention (C4).
//! - (DAG-depth/throughput correlation C1 is read from /health, not from this struct.)
//!
//! ## G1 safety
//! Everything here is an **observability side-channel only**: monotonic atomics + local
//! histograms. Nothing flows into Event/StateChange/VLC/state root. Timing is wall-clock
//! and is used for process-local reporting only.

use crate::histogram::{StageHistogram, StageStats};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Distribution summary. Unit depends on the metric: ns for latency/interval, raw count
/// for cf_size, raw depth for cold_parent_depth_diff. (Reuses `StageStats`; the `_ns`
/// field suffix is nominal for the non-latency distributions.)
pub type Dist = StageStats;

/// Events-per-second from a raw count over an elapsed nanosecond window.
/// Pure function. `elapsed_ns == 0` returns 0.0 (avoids div-by-zero on an empty window).
pub fn rate_per_sec(count: u64, elapsed_ns: u64) -> f64 {
    if elapsed_ns == 0 {
        return 0.0;
    }
    count as f64 * 1_000_000_000.0 / elapsed_ns as f64
}

/// Fraction `num / (num + other)` in [0.0, 1.0]. Pure function. Both zero -> 0.0.
/// Used for occ_rate = conflicts / (conflicts + applied), etc.
pub fn ratio(num: u64, other: u64) -> f64 {
    let denom = num + other;
    if denom == 0 {
        return 0.0;
    }
    num as f64 / denom as f64
}

/// Runtime accumulator for M1. One per run (reset between concurrency levels).
/// Counters are lock-free; distributions use a `Mutex<StageHistogram>` (CF/cold-parent
/// are low frequency; finalize-latency is per-applied-event but recorded under the
/// state-apply path that already serializes).
#[derive(Default)]
pub struct M1Metrics {
    events_applied: AtomicU64,
    occ_conflicts: AtomicU64,
    cold_parent_rejects: AtomicU64,
    cf_count: AtomicU64,
    finalize_latency_ns: Mutex<StageHistogram>,
    cf_size: Mutex<StageHistogram>,
    fold_interval_ns: Mutex<StageHistogram>,
    cold_parent_depth_diff: Mutex<StageHistogram>,
}

impl M1Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// A finalized (state-applied) event. `latency_ns == 0` means "no latency sample"
    /// (count only) — end-to-end finalize latency is otherwise covered by the M0
    /// finalization stages (FoldWait/Vote/Apply…), so callers may pass 0.
    pub fn record_applied(&self, latency_ns: u64) {
        self.events_applied.fetch_add(1, Ordering::Relaxed);
        if latency_ns > 0 {
            if let Ok(mut h) = self.finalize_latency_ns.lock() {
                h.record(latency_ns);
            }
        }
    }

    /// An OCC stale-read conflict (old_value mismatch -> event skipped at apply).
    pub fn record_occ_conflict(&self) {
        self.occ_conflicts.fetch_add(1, Ordering::Relaxed);
    }

    /// A cold-parent rejection ("Parent too old"), with its depth_diff.
    pub fn record_cold_parent(&self, depth_diff: u64) {
        self.cold_parent_rejects.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut h) = self.cold_parent_depth_diff.lock() {
            h.record(depth_diff);
        }
    }

    /// A ConsensusFrame was created folding `event_count` events; `interval_ns` is the
    /// wall-clock gap since the previous CF (0 for the first).
    pub fn record_cf(&self, event_count: u64, interval_ns: u64) {
        self.cf_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut h) = self.cf_size.lock() {
            h.record(event_count);
        }
        if let Ok(mut h) = self.fold_interval_ns.lock() {
            h.record(interval_ns);
        }
    }

    /// Snapshot derived metrics over an `elapsed_ns` measurement window.
    pub fn snapshot(&self, elapsed_ns: u64) -> M1Snapshot {
        let applied = self.events_applied.load(Ordering::Relaxed);
        let occ = self.occ_conflicts.load(Ordering::Relaxed);
        let cold = self.cold_parent_rejects.load(Ordering::Relaxed);
        let cfs = self.cf_count.load(Ordering::Relaxed);
        let stat = |m: &Mutex<StageHistogram>| m.lock().map(|mut h| h.stats()).unwrap_or(StageStats {
            count: 0, mean_ns: 0, p50_ns: 0, p95_ns: 0, p99_ns: 0, max_ns: 0,
        });
        M1Snapshot {
            events_applied: applied,
            occ_conflicts: occ,
            cold_parent_rejects: cold,
            cf_count: cfs,
            finalized_tps: rate_per_sec(applied, elapsed_ns),
            occ_rate: ratio(occ, applied),
            finalize_latency_ns: stat(&self.finalize_latency_ns),
            cf_size: stat(&self.cf_size),
            fold_interval_ns: stat(&self.fold_interval_ns),
            cold_parent_depth_diff: stat(&self.cold_parent_depth_diff),
        }
    }

    /// Reset all counters and distributions (between measurement windows).
    pub fn reset(&self) {
        self.events_applied.store(0, Ordering::Relaxed);
        self.occ_conflicts.store(0, Ordering::Relaxed);
        self.cold_parent_rejects.store(0, Ordering::Relaxed);
        self.cf_count.store(0, Ordering::Relaxed);
        for m in [
            &self.finalize_latency_ns,
            &self.cf_size,
            &self.fold_interval_ns,
            &self.cold_parent_depth_diff,
        ] {
            if let Ok(mut h) = m.lock() {
                *h = StageHistogram::default();
            }
        }
    }
}

/// Derived M1 metrics for a measurement window.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "enabled", derive(serde::Serialize))]
pub struct M1Snapshot {
    pub events_applied: u64,
    pub occ_conflicts: u64,
    pub cold_parent_rejects: u64,
    pub cf_count: u64,
    /// Finalized throughput: applied events / sec.
    pub finalized_tps: f64,
    /// OCC conflict fraction: conflicts / (conflicts + applied).
    pub occ_rate: f64,
    pub finalize_latency_ns: Dist,
    /// events-per-CF distribution (count, not ns).
    pub cf_size: Dist,
    pub fold_interval_ns: Dist,
    /// cold-parent depth_diff distribution (depth, not ns).
    pub cold_parent_depth_diff: Dist,
}

// ───────────────────────── global probe API (no-op when disabled) ─────────────────────────
//
// Probe sites in consensus/storage always call these free functions; when `enabled` is off
// they are `#[inline(always)]` no-ops (zero cost, no `#[cfg]` at the call sites). When on,
// they drive a process-global `M1Metrics` (thread-safe via atomics + Mutex; no background
// thread needed — counters are atomic and the distributions are low/serialized frequency).
pub use m1_imp::{
    m1_applied, m1_cf, m1_cold_parent, m1_init, m1_occ_conflict, m1_report_json, m1_reset,
    m1_snapshot,
};

#[cfg(feature = "enabled")]
mod m1_imp {
    use super::{M1Metrics, M1Snapshot};
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    static METRICS: OnceLock<M1Metrics> = OnceLock::new();
    /// Wall-clock of the previous CF, for computing fold intervals.
    static LAST_CF: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    /// Start of the current measurement window (set on reset/init), for finalized_tps.
    static WINDOW_START: OnceLock<Mutex<Instant>> = OnceLock::new();

    fn metrics() -> &'static M1Metrics {
        METRICS.get_or_init(M1Metrics::new)
    }
    fn last_cf() -> &'static Mutex<Option<Instant>> {
        LAST_CF.get_or_init(|| Mutex::new(None))
    }
    fn window_start() -> &'static Mutex<Instant> {
        WINDOW_START.get_or_init(|| Mutex::new(Instant::now()))
    }

    /// Idempotent init (touch the globals). Not calling it = zero overhead.
    pub fn m1_init() {
        let _ = metrics();
        let _ = last_cf();
        let _ = window_start();
    }

    /// A finalized (state-applied) event. `latency_ns == 0` = count only.
    #[inline]
    pub fn m1_applied(latency_ns: u64) {
        metrics().record_applied(latency_ns);
    }

    /// An OCC stale-read conflict (old_value mismatch -> event skipped at apply).
    #[inline]
    pub fn m1_occ_conflict() {
        metrics().record_occ_conflict();
    }

    /// A cold-parent rejection ("Parent too old"), with its depth_diff.
    #[inline]
    pub fn m1_cold_parent(depth_diff: u64) {
        metrics().record_cold_parent(depth_diff);
    }

    /// A ConsensusFrame folding `event_count` events; the fold interval is computed here
    /// from the previous CF's wall-clock.
    #[inline]
    pub fn m1_cf(event_count: u64) {
        let now = Instant::now();
        let interval_ns = last_cf()
            .lock()
            .ok()
            .and_then(|mut g| g.replace(now).map(|prev| now.saturating_duration_since(prev).as_nanos() as u64))
            .unwrap_or(0);
        metrics().record_cf(event_count, interval_ns);
    }

    /// Clear all counters/distributions and start a fresh measurement window.
    pub fn m1_reset() {
        metrics().reset();
        if let Ok(mut g) = last_cf().lock() {
            *g = None;
        }
        if let Ok(mut w) = window_start().lock() {
            *w = Instant::now();
        }
    }

    /// Snapshot derived M1 metrics over `elapsed_ns` (None only in disabled builds).
    pub fn m1_snapshot(elapsed_ns: u64) -> Option<M1Snapshot> {
        Some(metrics().snapshot(elapsed_ns))
    }

    /// Snapshot over the elapsed-since-reset window, serialized to a JSON object.
    pub fn m1_report_json() -> String {
        let elapsed_ns = window_start()
            .lock()
            .map(|w| w.elapsed().as_nanos() as u64)
            .unwrap_or(0);
        serde_json::to_string(&metrics().snapshot(elapsed_ns)).unwrap_or_default()
    }
}

#[cfg(not(feature = "enabled"))]
mod m1_imp {
    use super::M1Snapshot;

    #[inline(always)]
    pub fn m1_init() {}
    #[inline(always)]
    pub fn m1_applied(_latency_ns: u64) {}
    #[inline(always)]
    pub fn m1_occ_conflict() {}
    #[inline(always)]
    pub fn m1_cold_parent(_depth_diff: u64) {}
    #[inline(always)]
    pub fn m1_cf(_event_count: u64) {}
    #[inline(always)]
    pub fn m1_reset() {}
    #[inline(always)]
    pub fn m1_snapshot(_elapsed_ns: u64) -> Option<M1Snapshot> {
        None
    }
    #[inline(always)]
    pub fn m1_report_json() -> String {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T1 — events/sec over a ns window; div-by-zero guarded.
    #[test]
    fn t1_rate_per_sec() {
        assert_eq!(rate_per_sec(1000, 2_000_000_000), 500.0); // 1000 in 2s
        assert_eq!(rate_per_sec(0, 1_000_000_000), 0.0);
        assert_eq!(rate_per_sec(500, 0), 0.0); // empty window -> 0, no panic
    }

    /// T2 — ratio = num/(num+other), bounded, zero-safe.
    #[test]
    fn t2_ratio() {
        assert_eq!(ratio(3, 9), 0.25); // 3 conflicts vs 9 applied -> 25%
        assert_eq!(ratio(0, 100), 0.0);
        assert_eq!(ratio(0, 0), 0.0); // both zero -> 0, no panic
        assert_eq!(ratio(5, 0), 1.0);
    }

    /// T3 — record_* then snapshot produces the right counts, rates, distributions.
    #[test]
    fn t3_record_and_snapshot() {
        let m = M1Metrics::new();
        for i in 1..=10u64 {
            m.record_applied(i * 1_000_000); // latencies 1..10 ms
        }
        m.record_occ_conflict();
        m.record_occ_conflict();
        for d in [250u64, 300, 1000] {
            m.record_cold_parent(d);
        }
        m.record_cf(3, 0);
        m.record_cf(55, 1_000_000);
        m.record_cf(7, 2_000_000);

        let s = m.snapshot(1_000_000_000); // 1s window
        assert_eq!(s.events_applied, 10);
        assert_eq!(s.finalized_tps, 10.0); // 10 events / 1s
        assert_eq!(s.occ_conflicts, 2);
        assert_eq!(s.occ_rate, ratio(2, 10)); // 2/(2+10)
        assert_eq!(s.cold_parent_rejects, 3);
        assert_eq!(s.cold_parent_depth_diff.max_ns, 1000);
        assert_eq!(s.cf_count, 3);
        assert_eq!(s.cf_size.count, 3);
        assert_eq!(s.cf_size.max_ns, 55); // largest CF folded 55 events
        assert_eq!(s.finalize_latency_ns.count, 10);
        assert_eq!(s.finalize_latency_ns.max_ns, 10_000_000);
    }

    /// T4 — reset clears everything.
    #[test]
    fn t4_reset() {
        let m = M1Metrics::new();
        m.record_applied(5);
        m.record_occ_conflict();
        m.record_cold_parent(300);
        m.record_cf(10, 100);
        m.reset();
        let s = m.snapshot(1_000_000_000);
        assert_eq!(s.events_applied, 0);
        assert_eq!(s.occ_conflicts, 0);
        assert_eq!(s.cold_parent_rejects, 0);
        assert_eq!(s.cf_count, 0);
        assert_eq!(s.cf_size.count, 0);
        assert_eq!(s.finalize_latency_ns.count, 0);
    }
}
