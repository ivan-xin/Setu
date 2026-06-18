//! Finalization pipeline — pure decision logic (cf-finalization-cadence D1).
//!
//! The 3-stage pipeline (Stage-1 begin on router, Stage-2 apply worker, Stage-3
//! persist worker) decouples heavy finalization from the serial router loop.
//! This module holds the **pure, deterministic decision functions** the workers
//! call, isolated so they are unit-testable without the async/channel/lock
//! machinery. The wiring (channels, worker tasks, cross-crate persist) lives in
//! `engine.rs` (apply worker) and `setu-validator` (persist worker).
//!
//! Invariants encoded here:
//! - **INV-APPLY-ORDER** ([`apply_order_guard`]): apply by `anchor_chain_root`
//!   (NOT `depth==last+1`; `anchor.depth` is a DAG floor that can jump, F9/F10).
//! - **base-root parking** ([`parking_decision`]): a CF whose base chains off an
//!   in-flight (not-yet-finalized) parent is parked, not voted/caught-up.
//! - **INV-COMPLETION-ORDER** ([`drain_persisted_prefix`]): complete only the
//!   leading contiguous persisted prefix (R4-2), never a later-persisted entry
//!   ahead of an unpersisted one.
//! - **INV-ROUND-GATE** ([`can_fold_gate`]): no new fold while a prior CF is
//!   pending-build, applying, OR completing (R4-1).

/// What the Stage-2 apply worker does with a dequeued CF (INV-APPLY-ORDER).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyGuardDecision {
    /// Base matches the locally expected next chain root — apply now.
    Apply,
    /// Base chains off a known in-flight finalization — defer (park) and replay
    /// when the parent completes Stage-3.
    Defer,
    /// Base is neither current nor explainable by an in-flight — real gap → catch-up.
    CatchUp,
}

/// INV-APPLY-ORDER (R3, F9/F10). Accept a CF for apply iff its `anchor_chain_root`
/// equals the locally expected next chain root. `depth` is only a **monotonic
/// sanity** check — `anchor.depth` is a DAG floor that can jump (e.g. 0→55), so we
/// require `cf_depth >= last_applied_depth`, **never** `== last+1`.
///
/// - base == expected (and depth monotonic) → [`ApplyGuardDecision::Apply`]
/// - base == some in-flight parent's projected (after) root → [`ApplyGuardDecision::Defer`]
/// - otherwise → [`ApplyGuardDecision::CatchUp`]
pub fn apply_order_guard(
    expected_chain_root: [u8; 32],
    cf_chain_root: [u8; 32],
    last_applied_depth: u64,
    cf_depth: u64,
    in_flight_projected_roots: &[[u8; 32]],
) -> ApplyGuardDecision {
    if cf_chain_root == expected_chain_root {
        // Chain root says this is the next CF; `depth` is only a monotonic sanity
        // check (DAG floor can jump, F9). A regressing depth is anomalous → catch up.
        if cf_depth >= last_applied_depth {
            ApplyGuardDecision::Apply
        } else {
            ApplyGuardDecision::CatchUp
        }
    } else if in_flight_projected_roots.contains(&cf_chain_root) {
        ApplyGuardDecision::Defer
    } else {
        ApplyGuardDecision::CatchUp
    }
}

/// What the pre-vote / pre-enqueue path does with a received CF (base-root parking).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParkDecision {
    /// Base is the current committed chain root — proceed (vote / enqueue).
    Proceed,
    /// Base chains off an in-flight parent — park; replay when parent completes.
    Park,
}

/// base-root parking (D1.1b, R4-7). A CF whose `anchor_chain_root` matches the
/// **projected (after) root** of an in-flight finalization must be parked — it is
/// not stale (so not catch-up) and must not be voted/applied until its parent
/// finalizes (Stage-3). A CF matching the current committed root proceeds.
pub fn parking_decision(
    cf_chain_root: [u8; 32],
    current_committed_root: [u8; 32],
    in_flight_projected_roots: &[[u8; 32]],
) -> ParkDecision {
    if cf_chain_root == current_committed_root {
        ParkDecision::Proceed
    } else if in_flight_projected_roots.contains(&cf_chain_root) {
        ParkDecision::Park
    } else {
        // Neither current nor an in-flight projection: not a parking case (stale /
        // real gap is handled by the caller's normal/catch-up path).
        ParkDecision::Proceed
    }
}

/// INV-COMPLETION-ORDER (R4-2). Drain the **leading contiguous run** of persisted
/// entries from the head of a FIFO, leaving the first non-persisted entry (and
/// everything after it) in place — even if later entries are already persisted.
/// Returns the drained prefix in order.
pub fn drain_persisted_prefix<T>(
    queue: &mut std::collections::VecDeque<T>,
    is_persisted: impl Fn(&T) -> bool,
) -> Vec<T> {
    let mut drained = Vec::new();
    while let Some(front) = queue.front() {
        if is_persisted(front) {
            drained.push(queue.pop_front().expect("front just checked"));
        } else {
            break; // first unpersisted entry blocks the prefix — no skip-ahead.
        }
    }
    drained
}

/// INV-ROUND-GATE (R4-1) + BUG-010 one-pending-build. A new fold may start only
/// when there is no pending build, no CF mid-apply, AND no CF mid-completion
/// (Stage-2 cleared `applying` but Stage-3 has not yet advanced round).
pub fn can_fold_gate(pending_builds_empty: bool, applying_empty: bool, completing_empty: bool) -> bool {
    pending_builds_empty && applying_empty && completing_empty
}

/// One CF that has been *begun* (Stage-1) but has not yet completed Stage-3
/// (round-advance). Tracked so a child CF whose base chains off this one's
/// `projected_root` can be parked instead of triggering a false catch-up (D1.1b/R3).
#[derive(Debug, Clone)]
pub struct InFlightFinalization {
    pub cf_id: String,
    pub base_root: [u8; 32],
    /// The chain root the committed state will hold once this CF completes Stage-3
    /// (`chain_hash(before_root, anchor_hash)`, F18).
    pub projected_root: [u8; 32],
}

/// Stage-1 decision for a received CF (router thread, before begin/enqueue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage1Decision {
    /// Base is current → begin_finalization + enqueue to the apply worker.
    Begin,
    /// Base chains off an in-flight parent → park; replay when the parent completes.
    Park,
    /// Unexplainable gap → trigger catch-up.
    CatchUp,
    /// Already begun or parked (idempotent: duplicate vote / re-delivered CF / replay race).
    Skip,
}

/// The engine's finalization-pipeline decision state (cf-finalization-cadence D1).
/// Pure/`Send`, isolated from the async/lock machinery so the Stage-1 decision,
/// in-flight tracking, and deferred-replay logic are unit-testable. The engine holds
/// one of these (behind its lock) and the parked `ConsensusFrame` bodies in a separate
/// map keyed by `cf_id`.
#[derive(Debug, Default)]
pub struct PipelineState {
    /// Current committed chain root (advances only at Stage-3 complete).
    expected_chain_root: [u8; 32],
    /// Anchor depth of the last completed CF (monotonic sanity for the guard).
    last_applied_depth: u64,
    /// CFs begun but not yet Stage-3-completed.
    in_flight: Vec<InFlightFinalization>,
    /// cf_ids begun or parked — idempotency (do not double-begin/double-park).
    seen: std::collections::HashSet<String>,
    /// Parked cf_ids keyed by the base_root they are waiting on.
    deferred_by_base: std::collections::HashMap<[u8; 32], Vec<String>>,
}

impl PipelineState {
    /// New state at a known committed chain root + depth (genesis or post-recovery).
    pub fn new(expected_chain_root: [u8; 32], last_applied_depth: u64) -> Self {
        Self {
            expected_chain_root,
            last_applied_depth,
            ..Default::default()
        }
    }

    fn in_flight_projected(&self) -> Vec<[u8; 32]> {
        self.in_flight.iter().map(|f| f.projected_root).collect()
    }

    /// Stage-1 decision for a received CF. Idempotent (`Skip` if already begun/parked).
    pub fn decide(&self, cf_id: &str, cf_chain_root: [u8; 32], cf_depth: u64) -> Stage1Decision {
        if self.seen.contains(cf_id) {
            // Already begun or parked — idempotent (duplicate vote / re-delivered / replay race).
            return Stage1Decision::Skip;
        }
        match apply_order_guard(
            self.expected_chain_root,
            cf_chain_root,
            self.last_applied_depth,
            cf_depth,
            &self.in_flight_projected(),
        ) {
            ApplyGuardDecision::Apply => Stage1Decision::Begin,
            ApplyGuardDecision::Defer => Stage1Decision::Park,
            ApplyGuardDecision::CatchUp => Stage1Decision::CatchUp,
        }
    }

    /// Record that `cf_id` was begun (Stage-1) — registers it in-flight + marks seen.
    pub fn on_begin(&mut self, cf_id: String, base_root: [u8; 32], projected_root: [u8; 32]) {
        self.seen.insert(cf_id.clone());
        self.in_flight.push(InFlightFinalization { cf_id, base_root, projected_root });
    }

    /// Record that `cf_id` was parked (waiting on `base_root`) — marks seen.
    pub fn on_park(&mut self, cf_id: String, base_root: [u8; 32]) {
        self.seen.insert(cf_id.clone());
        self.deferred_by_base.entry(base_root).or_default().push(cf_id);
    }

    /// Record that the in-flight CF with `projected_root` completed Stage-3: advance the
    /// committed root to `projected_root` + `new_depth`, drop it from in-flight, and return
    /// the cf_ids parked on that root (now replayable — also un-`seen`ed so `decide` re-Begins).
    pub fn on_complete(&mut self, projected_root: [u8; 32], new_depth: u64) -> Vec<String> {
        // Stage-3 advanced the committed chain root + depth.
        self.expected_chain_root = projected_root;
        self.last_applied_depth = new_depth;
        // Drop the completed in-flight entry.
        if let Some(pos) = self.in_flight.iter().position(|f| f.projected_root == projected_root) {
            self.in_flight.remove(pos);
        }
        // CFs parked on this (now-current) root become replayable; un-`seen` them so
        // `decide` re-Begins (they were marked seen at park time).
        let replay = self.deferred_by_base.remove(&projected_root).unwrap_or_default();
        for id in &replay {
            self.seen.remove(id);
        }
        replay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    const R0: [u8; 32] = [0u8; 32];
    const R1: [u8; 32] = [1u8; 32];
    const R2: [u8; 32] = [2u8; 32];
    const R3: [u8; 32] = [3u8; 32];

    // ───────── INV-APPLY-ORDER (apply_order_guard) ─────────

    #[test]
    fn guard_applies_when_base_is_expected() {
        assert_eq!(apply_order_guard(R1, R1, 10, 11, &[]), ApplyGuardDecision::Apply);
    }

    #[test]
    fn guard_applies_on_depth_jump_when_chain_root_matches() {
        // F9: anchor.depth is a DAG floor that can jump 0→55; guard must NOT
        // require depth==last+1, only monotonic + chain-root match.
        assert_eq!(apply_order_guard(R1, R1, 0, 55, &[]), ApplyGuardDecision::Apply);
    }

    #[test]
    fn guard_defers_when_base_matches_in_flight_projected() {
        // CF chains off the projected root of an in-flight parent → park, not catch-up.
        assert_eq!(apply_order_guard(R1, R2, 10, 11, &[R2]), ApplyGuardDecision::Defer);
    }

    #[test]
    fn guard_catches_up_on_unexplainable_gap() {
        // base neither current-expected nor any in-flight projected → real gap.
        assert_eq!(apply_order_guard(R1, R3, 10, 12, &[R2]), ApplyGuardDecision::CatchUp);
    }

    #[test]
    fn guard_rejects_non_monotonic_depth() {
        // Even with matching chain root, a depth that regresses is a bug → not Apply.
        assert_ne!(apply_order_guard(R1, R1, 20, 10, &[]), ApplyGuardDecision::Apply);
    }

    // ───────── base-root parking (parking_decision) ─────────

    #[test]
    fn park_proceeds_on_current_root() {
        assert_eq!(parking_decision(R1, R1, &[R2]), ParkDecision::Proceed);
    }

    #[test]
    fn park_parks_on_in_flight_projected() {
        assert_eq!(parking_decision(R2, R1, &[R2]), ParkDecision::Park);
    }

    #[test]
    fn park_proceeds_when_no_in_flight_match_and_is_current() {
        assert_eq!(parking_decision(R1, R1, &[]), ParkDecision::Proceed);
    }

    // ───────── INV-COMPLETION-ORDER (drain_persisted_prefix) ─────────

    #[test]
    fn prefix_drain_takes_leading_persisted_run_only() {
        // [P, P, !P, P] → drain first two; the trailing persisted stays (blocked by gap).
        let mut q: VecDeque<(u64, bool)> =
            VecDeque::from(vec![(1, true), (2, true), (3, false), (4, true)]);
        let drained = drain_persisted_prefix(&mut q, |(_, p)| *p);
        assert_eq!(drained.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(q.len(), 2, "anchor3(unpersisted) + anchor4 stay (no skip-ahead)");
        assert_eq!(q.front().map(|(id, _)| *id), Some(3));
    }

    #[test]
    fn prefix_drain_empty_when_head_unpersisted() {
        // anchor2 persisted first but anchor1 not → drain nothing (R4-2: no skip).
        let mut q: VecDeque<(u64, bool)> = VecDeque::from(vec![(1, false), (2, true)]);
        let drained = drain_persisted_prefix(&mut q, |(_, p)| *p);
        assert!(drained.is_empty());
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn prefix_drain_takes_all_when_all_persisted() {
        let mut q: VecDeque<(u64, bool)> = VecDeque::from(vec![(1, true), (2, true), (3, true)]);
        let drained = drain_persisted_prefix(&mut q, |(_, p)| *p);
        assert_eq!(drained.len(), 3);
        assert!(q.is_empty());
    }

    // ───────── INV-ROUND-GATE (can_fold_gate) ─────────

    #[test]
    fn gate_open_only_when_all_clear() {
        assert!(can_fold_gate(true, true, true));
    }

    #[test]
    fn gate_closed_while_completing() {
        // R4-1: Stage-2 cleared applying, but Stage-3 has not advanced round → gate stays shut.
        assert!(!can_fold_gate(true, true, false));
    }

    #[test]
    fn gate_closed_while_pending_or_applying() {
        assert!(!can_fold_gate(false, true, true));
        assert!(!can_fold_gate(true, false, true));
    }

    // ───────── PipelineState (Stage-1 decision + in-flight tracking + replay) ─────────

    #[test]
    fn state_begins_on_current_base() {
        let s = PipelineState::new(R0, 10);
        assert_eq!(s.decide("cfN", R0, 11), Stage1Decision::Begin);
    }

    #[test]
    fn state_skips_duplicate_after_begin() {
        let mut s = PipelineState::new(R0, 10);
        s.on_begin("cfN".into(), R0, R1);
        // duplicate vote / re-delivered CF
        assert_eq!(s.decide("cfN", R0, 11), Stage1Decision::Skip);
    }

    #[test]
    fn state_parks_child_on_in_flight_projected() {
        let mut s = PipelineState::new(R0, 10);
        s.on_begin("cfN".into(), R0, R1); // N projects to R1, not yet completed
        // child N+1 chains off R1 → must park, not catch-up
        assert_eq!(s.decide("cfN1", R1, 12), Stage1Decision::Park);
    }

    #[test]
    fn state_catches_up_on_unexplainable_gap() {
        let mut s = PipelineState::new(R0, 10);
        s.on_begin("cfN".into(), R0, R1);
        assert_eq!(s.decide("cfGap", R3, 20), Stage1Decision::CatchUp);
    }

    #[test]
    fn state_complete_advances_and_returns_parked_for_replay() {
        let mut s = PipelineState::new(R0, 10);
        s.on_begin("cfN".into(), R0, R1);
        assert_eq!(s.decide("cfN1", R1, 12), Stage1Decision::Park);
        s.on_park("cfN1".into(), R1);
        // N completes Stage-3 → committed advances to R1, N1 becomes replayable
        let replay = s.on_complete(R1, 11);
        assert_eq!(replay, vec!["cfN1".to_string()]);
        // after replay un-seen, N1 now begins (base R1 == current)
        assert_eq!(s.decide("cfN1", R1, 12), Stage1Decision::Begin);
    }

    #[test]
    fn state_skips_parked_duplicate_before_completion() {
        let mut s = PipelineState::new(R0, 10);
        s.on_begin("cfN".into(), R0, R1);
        s.on_park("cfN1".into(), R1);
        // re-delivered while still parked → Skip (no double-park)
        assert_eq!(s.decide("cfN1", R1, 12), Stage1Decision::Skip);
    }

    #[test]
    fn state_complete_with_no_parked_returns_empty() {
        let mut s = PipelineState::new(R0, 10);
        s.on_begin("cfN".into(), R0, R1);
        assert!(s.on_complete(R1, 11).is_empty());
        // chain advanced: a fresh CF off R1 now begins
        assert_eq!(s.decide("cfN1", R1, 12), Stage1Decision::Begin);
    }
}
