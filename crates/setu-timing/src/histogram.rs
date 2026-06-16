//! Per-stage latency histogram and aggregator.
//!
//! The skeleton uses "sorted Vec + nearest-rank percentile" — simple and correct,
//! good enough for M0's offline aggregation. If memory becomes a problem under
//! sustained high TPS, switch to hdrhistogram (Open-2 to evaluate).

use crate::stage::StageId;
use std::collections::BTreeMap;

/// Nearest-rank percentile. `p` is in [0.0, 100.0]. Empty slice returns 0.
///
/// A pure function — feature-independent, always compiled, always testable.
pub fn percentile_ns(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    if p <= 0.0 {
        return sorted[0];
    }
    if p >= 100.0 {
        return sorted[sorted.len() - 1];
    }
    // Nearest-rank: rank = ceil(p/100 * N), 1-based.
    let n = sorted.len() as f64;
    let rank = (p / 100.0 * n).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

/// Sample bucket for a single stage.
#[derive(Default)]
pub struct StageHistogram {
    /// Raw nanosecond samples (sorted at report time).
    samples: Vec<u64>,
}

impl StageHistogram {
    pub fn record(&mut self, nanos: u64) {
        self.samples.push(nanos);
    }

    pub fn count(&self) -> usize {
        self.samples.len()
    }

    /// Compute stats (sorts `samples` in place).
    pub fn stats(&mut self) -> StageStats {
        self.samples.sort_unstable();
        let count = self.samples.len();
        let sum: u128 = self.samples.iter().map(|&x| x as u128).sum();
        let mean_ns = if count == 0 { 0 } else { (sum / count as u128) as u64 };
        StageStats {
            count,
            mean_ns,
            p50_ns: percentile_ns(&self.samples, 50.0),
            p95_ns: percentile_ns(&self.samples, 95.0),
            p99_ns: percentile_ns(&self.samples, 99.0),
            max_ns: self.samples.last().copied().unwrap_or(0),
        }
    }
}

/// Summary statistics for one stage.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "enabled", derive(serde::Serialize))]
pub struct StageStats {
    pub count: usize,
    pub mean_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
}

/// Aggregator across all stages. One per run / per concurrency level.
#[derive(Default)]
pub struct Aggregator {
    stages: BTreeMap<StageId, StageHistogram>,
}

impl Aggregator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, stage: StageId, nanos: u64) {
        self.stages.entry(stage).or_default().record(nanos);
    }

    /// Produce per-stage stats in pipeline order. Serial-wait stages (the saturation
    /// suspects) are flagged via `StageId::is_serial_wait`.
    pub fn report(&mut self) -> Vec<(StageId, StageStats)> {
        StageId::ALL
            .iter()
            .filter_map(|&s| self.stages.get_mut(&s).map(|h| (s, h.stats())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_basic() {
        let v: Vec<u64> = (1..=100).collect(); // 1..100
        assert_eq!(percentile_ns(&v, 50.0), 50);
        assert_eq!(percentile_ns(&v, 95.0), 95);
        assert_eq!(percentile_ns(&v, 99.0), 99);
        assert_eq!(percentile_ns(&v, 100.0), 100);
        assert_eq!(percentile_ns(&v, 0.0), 1);
    }

    #[test]
    fn percentile_empty_is_zero() {
        assert_eq!(percentile_ns(&[], 50.0), 0);
    }

    #[test]
    fn aggregator_separates_wait_and_work() {
        let mut agg = Aggregator::new();
        for i in 1..=10u64 {
            agg.record(StageId::ApplyWait, i * 1000); // large wait
            agg.record(StageId::ApplyWork, i * 10); // small work
        }
        let report = agg.report();
        let wait = report.iter().find(|(s, _)| *s == StageId::ApplyWait).unwrap().1;
        let work = report.iter().find(|(s, _)| *s == StageId::ApplyWork).unwrap().1;
        // Bisection holds: wait far exceeds work -> decision rule D6 points to A1.
        assert!(wait.p50_ns > work.p50_ns * 10);
    }
}
