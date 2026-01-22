//! Finalization Persistence
//!
//! Shared trait for persisting finalized consensus data (Anchor + Events).
//! This eliminates code duplication between MessageRouter and ConsensusValidator.

use consensus::ConsensusEngine;
use setu_storage::{AnchorStore, EventStore};
use setu_types::Anchor;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Trait for components that can persist finalized anchors
///
/// Both `MessageRouter` and `ConsensusValidator` need to persist finalized data
/// when a CF reaches quorum. This trait provides a shared implementation.
///
/// ## Persistence Order
/// 
/// Events are persisted first, then Anchor (as commit marker).
/// This ensures crash recovery can detect incomplete persistence.
#[async_trait::async_trait]
pub trait FinalizationPersister: Send + Sync {
    /// Get the consensus engine (for fetching events by ID)
    fn engine(&self) -> &Arc<ConsensusEngine>;
    
    /// Get the event store (for persisting events)
    fn event_store(&self) -> &Arc<EventStore>;
    
    /// Get the anchor store (for persisting anchors)
    fn anchor_store(&self) -> &Arc<AnchorStore>;

    /// Persist a finalized anchor and its events to storage
    /// 
    /// Order: Events first, then Anchor (anchor serves as commit marker)
    /// This ensures that if we crash after persisting events but before anchor,
    /// we can detect incomplete persistence and retry.
    async fn persist_finalized_anchor(&self, anchor: &Anchor) {
        // 1. Get all events included in this anchor from the DAG
        let events = self.engine().get_events_by_ids(&anchor.event_ids).await;
        
        // 2. Batch persist events to EventStore (before anchor)
        let mut stored_count = 0;
        for event in &events {
            if let Err(e) = self.event_store().store(event.clone()).await {
                warn!(
                    event_id = %event.id,
                    anchor_id = %anchor.id,
                    error = %e,
                    "Failed to persist finalized event"
                );
            } else {
                stored_count += 1;
            }
        }
        
        if !events.is_empty() {
            debug!(
                anchor_id = %anchor.id,
                stored_events = stored_count,
                total_events = anchor.event_ids.len(),
                "Persisted finalized events"
            );
        }
        
        // 3. Persist anchor to AnchorStore (commit marker)
        if let Err(e) = self.anchor_store().store(anchor.clone()).await {
            warn!(
                anchor_id = %anchor.id,
                error = %e,
                "Failed to persist finalized anchor"
            );
        } else {
            info!(anchor_id = %anchor.id, "Persisted finalized anchor");
        }
        
        // 4. Mark the anchor as persisted in engine (allows GC of in-memory data)
        self.engine().mark_anchor_persisted(&anchor.id).await;
    }
}
