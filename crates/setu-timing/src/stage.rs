//! Pipeline stage identifiers.
//!
//! Mirrors the 11-stage decomposition in `docs/feat/m0-pipeline-baseline/design.md` §D1;
//! fold and apply are each split into wait + work (the "wait vs work" bisection of §D2),
//! so the enum has 13 segments. In the aggregator, wait/work are just two ordinary
//! stages, so the bisection falls out naturally.

/// A measurable segment of a transaction's pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "enabled", derive(serde::Serialize, serde::Deserialize))]
pub enum StageId {
    /// 1. RPC parse + admission signature verification
    Ingress,
    /// 2. Coin reservation (coin_reservation::try_reserve)
    Reserve,
    /// 3. Coin selection + Merkle proof (task_preparer)
    Prep,
    /// 4. Solver selection (router_manager)
    Route,
    /// 5. HTTP to solver + queue wait (from before-spawn to execution start, see Open-4)
    Dispatch,
    /// 6. TEE execute_stf (reuses StfResultDto.execution_time_us)
    Tee,
    /// 7. verify_id + add_event into the DAG
    Submit,
    /// 7a. (submit breakdown) verify_id anti-tamper hash recompute
    SubmitVerify,
    /// 7b. (submit breakdown) VLC lock + merge + tick
    SubmitVlc,
    /// 7c. (submit breakdown) DAG insertion (add_event_with_retry: lock + parent resolution)
    SubmitDag,
    /// 7d. (submit breakdown) synchronous P2P broadcast to peers (prime suspect for the ~100ms submit cost)
    SubmitBroadcast,
    /// 8a. Event queued in the DAG waiting to be folded into a CF (bottleneck ②: leader per-round serialization)
    FoldWait,
    /// 8b. The CF-folding work itself
    FoldWork,
    /// 9. Voting + reaching quorum (incl. network RTT; multi-node only)
    Vote,
    /// 10a. Waiting for the global apply lock write_gsm (bottleneck ③: blocked by other subnets)
    ApplyWait,
    /// 10b. Applying state while holding the lock
    ApplyWork,
    /// 11. Anchor persistence WriteBatch
    Commit,
    // ───────── P0 finalization-pipeline instrumentation (cf-finalization-cadence) ─────────
    // All measure the CURRENT inline finalization path (pipeline not yet built), to predict
    // the post-D1 per-CF serial floor. See docs/feat/cf-finalization-cadence/p0-instrumentation-plan.md.
    /// Time an event spends queued in the router channel before `route_event` picks it up
    /// (head-of-line backlog evidence — Q0).
    RouterWait,
    /// Single `route_event` handling duration (grows when inline finalization runs — Q0).
    RouteEvent,
    /// `ensure_cf_events_available` inline fetch before voting (Q3).
    EventFetch,
    /// `post_finalize_*` tail (write pending_* / completion) (Q1 Floor-A).
    PostFinalize,
    /// `persist_pending_finalized_cfs` CF-index persist (Q1 / R5-5 global-drain magnitude).
    PersistCfIndex,
    /// `anchor_store().store` anchor commit-marker persist (Q1 / R5-4).
    PersistAnchor,
    /// Window between CF-index visible and anchor durable (`mark_anchor_persisted`) — R5-4 hazard size.
    DurabilityGap,
    /// `complete_pending_finalizations` (broadcast finalized + advance round) (Q1 Floor-A).
    Complete,
    /// Leader fold-gate closed duration: finalize trigger → next `can_start_new_pending_build` true (Q0/Q2).
    GateClosed,
}

impl StageId {
    /// All segments, in pipeline order (incl. the 4 submit-breakdown sub-stages
    /// and the 9 P0 finalization-pipeline probes).
    pub const ALL: [StageId; 26] = [
        StageId::Ingress,
        StageId::Reserve,
        StageId::Prep,
        StageId::Route,
        StageId::Dispatch,
        StageId::Tee,
        StageId::Submit,
        StageId::SubmitVerify,
        StageId::SubmitVlc,
        StageId::SubmitDag,
        StageId::SubmitBroadcast,
        StageId::FoldWait,
        StageId::FoldWork,
        StageId::Vote,
        StageId::ApplyWait,
        StageId::ApplyWork,
        StageId::Commit,
        StageId::RouterWait,
        StageId::RouteEvent,
        StageId::EventFetch,
        StageId::PostFinalize,
        StageId::PersistCfIndex,
        StageId::PersistAnchor,
        StageId::DurabilityGap,
        StageId::Complete,
        StageId::GateClosed,
    ];

    /// Stable short name (report column / jsonl field).
    pub const fn name(self) -> &'static str {
        match self {
            StageId::Ingress => "ingress",
            StageId::Reserve => "reserve",
            StageId::Prep => "prep",
            StageId::Route => "route",
            StageId::Dispatch => "dispatch",
            StageId::Tee => "tee",
            StageId::Submit => "submit",
            StageId::SubmitVerify => "submit_verify",
            StageId::SubmitVlc => "submit_vlc",
            StageId::SubmitDag => "submit_dag",
            StageId::SubmitBroadcast => "submit_broadcast",
            StageId::FoldWait => "fold_wait",
            StageId::FoldWork => "fold_work",
            StageId::Vote => "vote",
            StageId::ApplyWait => "apply_wait",
            StageId::ApplyWork => "apply_work",
            StageId::Commit => "commit",
            StageId::RouterWait => "router_wait",
            StageId::RouteEvent => "route_event",
            StageId::EventFetch => "event_fetch",
            StageId::PostFinalize => "post_finalize",
            StageId::PersistCfIndex => "persist_cf_index",
            StageId::PersistAnchor => "persist_anchor",
            StageId::DurabilityGap => "durability_gap",
            StageId::Complete => "complete",
            StageId::GateClosed => "gate_closed",
        }
    }

    /// Whether this segment is the "wait on a serial resource" — the M0 decision core (§D6):
    /// if these grow with concurrency / subnet count, the corresponding parallelization
    /// (A1/A2) is justified.
    pub const fn is_serial_wait(self) -> bool {
        matches!(self, StageId::FoldWait | StageId::ApplyWait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_unique_and_total() {
        assert_eq!(StageId::ALL.len(), 26);
        let mut names: Vec<&str> = StageId::ALL.iter().map(|s| s.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 26, "stage names must be unique");
    }

    #[test]
    fn serial_waits_are_the_two_bisected_stages() {
        let waits: Vec<_> = StageId::ALL
            .iter()
            .copied()
            .filter(|s| s.is_serial_wait())
            .collect();
        assert_eq!(waits, vec![StageId::FoldWait, StageId::ApplyWait]);
    }
}
