//! Collector: feature-gated side-channel.
//!
//! Design notes (docs/feat/m0-pipeline-baseline §D3 / R1-ISSUE-3/4):
//! - **No-op guard pattern**: probe sites always call `Span::start(..)` / `record(..)`;
//!   when the feature is off they are `#[inline(always)]` no-ops -> zero cost, with no
//!   `#[cfg]` sprinkled across hot paths.
//! - **MPSC + background aggregation**: the hot path only does `Instant::now()` + a channel
//!   send, holding **no shared lock** — otherwise the measurement itself would worsen the
//!   very apply-lock contention it is trying to measure (observer effect).
//! - **G1 safety**: `Instant`/samples flow only into this collector (side-channel); they
//!   **never** enter `Event`/`StateChange`/`VLC`/state root.

use crate::histogram::StageStats;
use crate::stage::StageId;
use std::sync::atomic::{AtomicU64, Ordering};

/// Correlation id for a single transaction.
///
/// Two sources:
/// - `from_hex(event_id_hex)`: after submit, when the event has a stable id.
/// - `next()`: in the early ingress stages (before the event id exists); correlating
///   the pre-submit and post-submit traces is the caller's responsibility (see design Open-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceId(pub u64);

static COUNTER: AtomicU64 = AtomicU64::new(1);

impl TraceId {
    /// Take the first 16 hex chars of the event id -> u64 (stable, identical across nodes).
    /// Falls back to 0 on parse failure.
    pub fn from_hex(event_id_hex: &str) -> Self {
        let s = event_id_hex.strip_prefix("0x").unwrap_or(event_id_hex);
        let head: String = s.chars().take(16).collect();
        TraceId(u64::from_str_radix(&head, 16).unwrap_or(0))
    }

    /// Process-local monotonically increasing correlation id (for the early pre-submit stages).
    pub fn next() -> Self {
        TraceId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

pub use imp::{init, mark, measure_from, record, report, reset, Span};

// ───────────────────────── enabled: real implementation ─────────────────────────
#[cfg(feature = "enabled")]
mod imp {
    use super::*;
    use crate::histogram::Aggregator;
    use std::collections::HashMap;
    use std::sync::mpsc::{channel, Sender};
    use std::sync::OnceLock;
    use std::time::Instant;

    enum Msg {
        /// A same-scope span whose duration is already computed.
        Span { stage: StageId, nanos: u64 },
        /// Cross-site start timestamp (e.g. fold_wait at add_event).
        Mark { tid: TraceId, stage: StageId, at: Instant },
        /// Cross-site end timestamp (e.g. fold_wait at fold); recorded once matched with its Mark.
        Measure { tid: TraceId, stage: StageId, at: Instant },
        /// Pull the current aggregation snapshot.
        Report(Sender<Vec<(StageId, StageStats)>>),
        /// Clear all samples + pending marks (start a fresh measurement window).
        Reset,
    }

    static TX: OnceLock<Sender<Msg>> = OnceLock::new();

    /// Start the background aggregation thread (idempotent). Not calling it in production
    /// means zero overhead.
    pub fn init() {
        TX.get_or_init(|| {
            let (tx, rx) = channel::<Msg>();
            std::thread::Builder::new()
                .name("m0-aggregator".into())
                .spawn(move || {
                    let mut agg = Aggregator::new();
                    // Cross-site start points: held single-threaded, lock-free.
                    let mut marks: HashMap<(TraceId, StageId), Instant> = HashMap::new();
                    while let Ok(msg) = rx.recv() {
                        match msg {
                            Msg::Span { stage, nanos } => agg.record(stage, nanos),
                            Msg::Mark { tid, stage, at } => {
                                marks.insert((tid, stage), at);
                            }
                            Msg::Measure { tid, stage, at } => {
                                if let Some(start) = marks.remove(&(tid, stage)) {
                                    let ns =
                                        at.saturating_duration_since(start).as_nanos() as u64;
                                    agg.record(stage, ns);
                                }
                            }
                            Msg::Report(reply) => {
                                let _ = reply.send(agg.report());
                            }
                            Msg::Reset => {
                                agg = Aggregator::new();
                                marks.clear();
                            }
                        }
                    }
                })
                .expect("spawn m0-aggregator");
            tx
        });
    }

    #[inline]
    fn send(msg: Msg) {
        if let Some(tx) = TX.get() {
            let _ = tx.send(msg); // ignore errors: an exited aggregator must not affect the measured path
        }
    }

    /// Record a segment with a known duration directly (e.g. TEE reusing execution_time_us).
    #[inline]
    pub fn record(stage: StageId, nanos: u64) {
        send(Msg::Span { stage, nanos });
    }

    /// Cross-site start point (e.g. fold_wait when the event enters the DAG).
    #[inline]
    pub fn mark(tid: TraceId, stage: StageId) {
        send(Msg::Mark { tid, stage, at: Instant::now() });
    }

    /// Cross-site end point (e.g. fold_wait when the event is folded into a CF).
    #[inline]
    pub fn measure_from(tid: TraceId, stage: StageId) {
        send(Msg::Measure { tid, stage, at: Instant::now() });
    }

    /// Clear all samples + pending marks (start a fresh measurement window).
    pub fn reset() {
        send(Msg::Reset);
    }

    /// Pull the per-stage stats snapshot (call at end of run).
    pub fn report() -> Vec<(StageId, StageStats)> {
        let (rtx, rrx) = channel();
        send(Msg::Report(rtx));
        rrx.recv().unwrap_or_default()
    }

    /// Same-scope RAII timer; records `elapsed` on drop.
    pub struct Span {
        stage: StageId,
        start: Instant,
    }

    impl Span {
        #[inline]
        pub fn start(stage: StageId, _tid: TraceId) -> Self {
            // tid is unnecessary for a same-scope span (no cross-site matching needed);
            // kept only for API consistency.
            Span { stage, start: Instant::now() }
        }
    }

    impl Drop for Span {
        fn drop(&mut self) {
            let ns = self.start.elapsed().as_nanos() as u64;
            record(self.stage, ns);
        }
    }
}

// ───────────────────────── disabled: zero-cost no-op ─────────────────────────
#[cfg(not(feature = "enabled"))]
mod imp {
    use super::*;

    #[inline(always)]
    pub fn init() {}
    #[inline(always)]
    pub fn record(_stage: StageId, _nanos: u64) {}
    #[inline(always)]
    pub fn mark(_tid: TraceId, _stage: StageId) {}
    #[inline(always)]
    pub fn measure_from(_tid: TraceId, _stage: StageId) {}
    #[inline(always)]
    pub fn reset() {}
    #[inline(always)]
    pub fn report() -> Vec<(StageId, StageStats)> {
        Vec::new()
    }

    /// Zero-sized guard; start/drop are both no-ops (eliminated by the compiler).
    pub struct Span;
    impl Span {
        #[inline(always)]
        pub fn start(_stage: StageId, _tid: TraceId) -> Self {
            Span
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_id_from_hex_stable() {
        let a = TraceId::from_hex("00000000000000ff_deadbeef".replace('_', "").as_str());
        assert_eq!(a, TraceId(0x00000000000000ff));
        // with 0x prefix
        assert_eq!(TraceId::from_hex("0x0000000000000001"), TraceId(1));
        // invalid falls back to 0
        assert_eq!(TraceId::from_hex("zzzz"), TraceId(0));
    }

    #[test]
    fn trace_id_next_monotonic() {
        let a = TraceId::next();
        let b = TraceId::next();
        assert!(b.0 > a.0);
    }

    // Under a disabled build, the calls below must compile and be no-ops
    // (no panic, no dependency on init).
    #[test]
    fn noop_api_is_callable() {
        init();
        let tid = TraceId::next();
        {
            let _g = Span::start(StageId::Ingress, tid);
        }
        record(StageId::Tee, 123);
        mark(tid, StageId::FoldWait);
        measure_from(tid, StageId::FoldWait);
        let _ = report();
    }
}
