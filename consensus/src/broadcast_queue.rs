//! Bounded FIFO event-broadcast queue (async-event-broadcast design D4).
//!
//! Moves the per-event P2P broadcast off the `add_event` critical path
//! (M0-report §9/§10: the synchronous `broadcast_event().await` was ~94% of the
//! submit cost). `enqueue` is a non-blocking `try_send`; a single worker drains in
//! FIFO order and broadcasts. FIFO preserves ordering (so peers don't see spurious
//! `MissingParent`); the bounded channel + drop-on-full bounds memory (broadcast is
//! best-effort, gaps recovered via state sync / DAG replay).
//!
//! G1/G2: broadcast is a network side-effect only — it never touches state / state root.

use crate::broadcaster::ConsensusBroadcaster;
use setu_types::Event;
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::TrySendError};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// Slot holding the (optionally late-set) broadcaster, shared with the worker.
type BroadcasterSlot = Arc<RwLock<Option<Arc<dyn ConsensusBroadcaster>>>>;

/// Outcome of a non-blocking enqueue (for metrics / logging).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// Event accepted into the queue.
    Enqueued,
    /// Queue full — event dropped (best-effort; recovered via state sync).
    DroppedFull,
    /// Worker gone / channel closed.
    Closed,
}

/// Handle to the bounded FIFO broadcast queue. Held by `ConsensusEngine`.
pub struct BroadcastQueue {
    tx: mpsc::Sender<Event>,
    /// Worker handle. In production the task runs detached (it exits when `tx` drops on
    /// engine teardown); the handle is read only by tests (`drain_and_join`). Held to keep
    /// ownership tidy.
    #[allow(dead_code)]
    worker: JoinHandle<()>,
}

impl BroadcastQueue {
    /// Create the bounded channel and spawn the single FIFO drain worker.
    pub fn spawn(broadcaster_slot: BroadcasterSlot, capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel::<Event>(capacity.max(1));
        let worker = tokio::spawn(drain_loop(rx, broadcaster_slot));
        Self { tx, worker }
    }

    /// Non-blocking enqueue. Never blocks the caller (the submit critical path).
    pub fn enqueue(&self, event: Event) -> EnqueueOutcome {
        match self.tx.try_send(event) {
            Ok(()) => EnqueueOutcome::Enqueued,
            Err(TrySendError::Full(_)) => EnqueueOutcome::DroppedFull,
            Err(TrySendError::Closed(_)) => EnqueueOutcome::Closed,
        }
    }

    /// Test helper: close the sender and wait for the worker to drain + exit.
    #[cfg(test)]
    pub async fn drain_and_join(self) {
        let BroadcastQueue { tx, worker } = self;
        drop(tx);
        let _ = worker.await;
    }
}

/// Single FIFO worker: drains the channel in order and broadcasts each event.
///
/// Strict FIFO (one worker, awaits each broadcast before the next `recv`) preserves
/// broadcast ordering. Reads the broadcaster slot each iteration so a late-set / `None`
/// broadcaster is handled without panic. Errors are logged, not fatal (best-effort;
/// any drops are recovered when a CF references the event via `request_events`).
///
/// Throughput note: a single worker serializes broadcasts. The ~111ms seen on the
/// synchronous path was contention from many concurrent broadcasts; serializing should
/// remove that contention. Adequacy is an empirical question — measured on the cluster.
async fn drain_loop(mut rx: mpsc::Receiver<Event>, slot: BroadcasterSlot) {
    while let Some(event) = rx.recv().await {
        let broadcaster = { slot.read().await.clone() };
        if let Some(b) = broadcaster {
            if let Err(e) = b.broadcast_event(&event).await {
                tracing::warn!(event_id = %event.id, error = %e, "Failed to broadcast event (async worker)");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broadcaster::{BroadcastError, BroadcastResult};
    use setu_types::{ConsensusFrame, EventId, Vote};
    use std::sync::Mutex;
    use std::time::Duration;

    /// Recording mock: captures broadcast_event order; optional per-call delay (T6).
    #[derive(Debug)]
    struct RecordingBroadcaster {
        order: Arc<Mutex<Vec<EventId>>>,
        delay: Option<Duration>,
    }

    impl RecordingBroadcaster {
        fn new(order: Arc<Mutex<Vec<EventId>>>, delay: Option<Duration>) -> Self {
            Self { order, delay }
        }
    }

    #[async_trait::async_trait]
    impl ConsensusBroadcaster for RecordingBroadcaster {
        async fn broadcast_cf(&self, _cf: &ConsensusFrame) -> Result<BroadcastResult, BroadcastError> {
            Ok(BroadcastResult::success(0, 0))
        }
        async fn broadcast_vote(&self, _vote: &Vote) -> Result<BroadcastResult, BroadcastError> {
            Ok(BroadcastResult::success(0, 0))
        }
        async fn broadcast_finalized(&self, _cf: &ConsensusFrame) -> Result<BroadcastResult, BroadcastError> {
            Ok(BroadcastResult::success(0, 0))
        }
        async fn broadcast_event(&self, event: &Event) -> Result<BroadcastResult, BroadcastError> {
            if let Some(d) = self.delay {
                tokio::time::sleep(d).await;
            }
            self.order.lock().unwrap().push(event.id.clone());
            Ok(BroadcastResult::success(0, 0))
        }
        async fn request_events(&self, _ids: &[EventId]) -> Result<Vec<Event>, BroadcastError> {
            Ok(Vec::new())
        }
        fn peer_count(&self) -> usize { 0 }
        fn local_validator_id(&self) -> &str { "test" }
    }

    fn slot_with(b: Option<Arc<dyn ConsensusBroadcaster>>) -> BroadcasterSlot {
        Arc::new(RwLock::new(b))
    }

    fn ev(id: &str) -> Event {
        let mut e = Event::new(
            setu_types::EventType::Transfer,
            vec![],
            setu_types::VLCSnapshot::new(),
            "test".to_string(),
        );
        e.id = id.to_string();
        e
    }

    // T1: enqueue under capacity -> Enqueued
    #[tokio::test]
    async fn t1_enqueue_under_capacity() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let b: Arc<dyn ConsensusBroadcaster> = Arc::new(RecordingBroadcaster::new(order, None));
        let q = BroadcastQueue::spawn(slot_with(Some(b)), 8);
        assert_eq!(q.enqueue(ev("e1")), EnqueueOutcome::Enqueued);
    }

    // T2: enqueue when full -> DroppedFull (non-blocking, no panic)
    #[tokio::test]
    async fn t2_enqueue_full_drops() {
        // Blocking broadcaster so the worker can't drain; tiny capacity fills fast.
        let order = Arc::new(Mutex::new(Vec::new()));
        let b: Arc<dyn ConsensusBroadcaster> =
            Arc::new(RecordingBroadcaster::new(order, Some(Duration::from_secs(30))));
        let q = BroadcastQueue::spawn(slot_with(Some(b)), 1);
        let mut dropped = 0;
        for i in 0..50 {
            if q.enqueue(ev(&format!("e{i}"))) == EnqueueOutcome::DroppedFull {
                dropped += 1;
            }
        }
        assert!(dropped > 0, "full queue must drop, got 0 drops");
    }

    // T3: FIFO order preserved (the core property)
    #[tokio::test]
    async fn t3_fifo_order() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let b: Arc<dyn ConsensusBroadcaster> =
            Arc::new(RecordingBroadcaster::new(order.clone(), None));
        let q = BroadcastQueue::spawn(slot_with(Some(b)), 64);
        for i in 0..20 {
            assert_eq!(q.enqueue(ev(&format!("e{i}"))), EnqueueOutcome::Enqueued);
        }
        q.drain_and_join().await;
        let got = order.lock().unwrap().clone();
        let want: Vec<String> = (0..20).map(|i| format!("e{i}")).collect();
        assert_eq!(got, want, "broadcasts must be FIFO");
    }

    // T4: enqueue after worker/channel closed -> Closed
    #[tokio::test]
    async fn t4_closed() {
        let q = BroadcastQueue::spawn(slot_with(None), 4);
        // Drop the worker's receiver by aborting the worker, then closing.
        q.worker.abort();
        // Give the runtime a tick so the receiver is dropped.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(q.enqueue(ev("e1")), EnqueueOutcome::Closed);
    }

    // T5: None broadcaster slot -> drained without panic (no-op broadcast)
    #[tokio::test]
    async fn t5_none_broadcaster_no_panic() {
        let q = BroadcastQueue::spawn(slot_with(None), 8);
        for i in 0..5 {
            assert_eq!(q.enqueue(ev(&format!("e{i}"))), EnqueueOutcome::Enqueued);
        }
        q.drain_and_join().await; // must not panic
    }

    // T6: enqueue is non-blocking even when the worker is slow/stuck
    #[tokio::test]
    async fn t6_enqueue_non_blocking() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let b: Arc<dyn ConsensusBroadcaster> =
            Arc::new(RecordingBroadcaster::new(order, Some(Duration::from_secs(30))));
        let q = BroadcastQueue::spawn(slot_with(Some(b)), 1);
        let start = tokio::time::Instant::now();
        for i in 0..100 {
            let _ = q.enqueue(ev(&format!("e{i}")));
        }
        // 100 enqueues against a stuck worker must return ~instantly (non-blocking).
        assert!(start.elapsed() < Duration::from_secs(1), "enqueue blocked");
    }
}
