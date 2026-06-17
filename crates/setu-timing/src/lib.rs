//! # setu-timing — observability side-channel for M0 pipeline profiling
//!
//! See `docs/feat/m0-pipeline-baseline/design.md`. This crate provides per-stage latency
//! collection and aggregation to answer "which stage is the end-to-end bottleneck" (the
//! S2 G2 gate).
//!
//! ## Two inviolable constraints
//!
//! 1. **G1 safety (determinism)**: everything in this crate (`Instant`, samples, `StageId`,
//!    stats) flows **only into the observability side-channel** and **never** into `Event` /
//!    `StateChange` / `VLC` / state root. Timing is wall-clock; leaking it into state would
//!    diverge validators' state roots and break consensus.
//!
//! 2. **Architecture guard**: this crate is a **pure leaf** (zero `setu-*` deps). **Do not**
//!    add it to the dependencies of `types/` (G4: may depend only on setu-vlc) or
//!    `crates/setu-runtime/` (G5: may depend only on setu-types). Probes live only in
//!    storage/consensus/setu-validator/setu-enclave.
//!
//! ## Zero cost
//!
//! The feature is off by default -> all APIs are `#[inline(always)]` no-ops and `Span` is a
//! zero-sized type. Production builds carry zero probes and zero external deps. Consuming
//! crates opt in via their own `m0-profiling = ["setu-timing/enabled"]`.
//!
//! ## Usage (probe sites, wired in the next sub-step)
//!
//! ```ignore
//! use setu_timing::{Span, StageId, TraceId, mark, measure_from, record};
//!
//! // Same-scope segment (records on drop):
//! let tid = TraceId::from_hex(&event.id);
//! { let _g = Span::start(StageId::ApplyWork, tid); /* apply while holding the lock */ }
//!
//! // Cross-site segment (fold_wait: mark at add_event, measure at fold):
//! mark(TraceId::from_hex(&event.id), StageId::FoldWait);   // when entering the DAG
//! // ... later ...
//! measure_from(TraceId::from_hex(&event.id), StageId::FoldWait); // when folding the CF
//!
//! // Reuse an already-known duration (TEE reports execution_time_us):
//! record(StageId::Tee, execution_time_us * 1000);
//! ```

mod collector;
mod histogram;
mod m1;
mod stage;

pub use collector::{init, mark, measure_from, record, report, reset, Span, TraceId};
pub use histogram::{percentile_ns, Aggregator, StageHistogram, StageStats};
pub use m1::{
    m1_applied, m1_cf, m1_cold_parent, m1_init, m1_occ_conflict, m1_report_json, m1_reset,
    m1_snapshot, rate_per_sec, ratio, Dist, M1Metrics, M1Snapshot,
};
pub use stage::StageId;

/// Render the per-stage stats snapshot as jsonl (one stage per line), for offline
/// aggregation / the M0 report. Produces real content only when `enabled`; otherwise
/// returns an empty string (zero deps).
#[cfg(feature = "enabled")]
pub fn report_jsonl() -> String {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Row {
        stage: &'static str,
        #[serde(flatten)]
        stats: StageStats,
    }
    report()
        .into_iter()
        .map(|(s, stats)| {
            let row = Row { stage: s.name(), stats };
            serde_json::to_string(&row).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Disabled build: placeholder, returns an empty string.
#[cfg(not(feature = "enabled"))]
pub fn report_jsonl() -> String {
    String::new()
}
