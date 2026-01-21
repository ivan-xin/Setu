//! Anemo-based Consensus Broadcaster
//!
//! This adapter wraps the AnemoNetworkService to implement the
//! ConsensusBroadcaster trait, providing actual P2P message delivery
//! for consensus messages.

use consensus::{BroadcastError, BroadcastResult, ConsensusBroadcaster};
use setu_network_anemo::AnemoNetworkService;
use setu_types::{ConsensusFrame, Event, EventId, Vote};
use std::fmt;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Adapter that implements ConsensusBroadcaster using Anemo P2P network
pub struct AnemoConsensusBroadcaster {
    /// The underlying Anemo network service
    network: Arc<AnemoNetworkService>,
    /// Local validator ID
    local_validator_id: String,
}

impl fmt::Debug for AnemoConsensusBroadcaster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnemoConsensusBroadcaster")
            .field("local_validator_id", &self.local_validator_id)
            .field("peer_count", &self.network.get_peer_count())
            .finish()
    }
}

impl AnemoConsensusBroadcaster {
    /// Create a new Anemo consensus broadcaster
    pub fn new(network: Arc<AnemoNetworkService>, local_validator_id: String) -> Self {
        Self {
            network,
            local_validator_id,
        }
    }
}

#[async_trait::async_trait]
impl ConsensusBroadcaster for AnemoConsensusBroadcaster {
    async fn broadcast_cf(&self, cf: &ConsensusFrame) -> Result<BroadcastResult, BroadcastError> {
        let peers = self.network.get_connected_peers();
        let total_peers = peers.len();
        
        if total_peers == 0 {
            debug!(cf_id = %cf.id, "No peers to broadcast CF to");
            return Ok(BroadcastResult::success(0, 0));
        }

        let mut success_count = 0;
        let mut failures = Vec::new();

        for peer_id in peers {
            match self.network.send_cf_proposal(&peer_id, cf.clone()).await {
                Ok(_) => {
                    success_count += 1;
                    debug!(cf_id = %cf.id, peer = %peer_id, "CF sent to peer");
                }
                Err(e) => {
                    let error_msg = format!("{}", e);
                    warn!(cf_id = %cf.id, peer = %peer_id, error = %error_msg, "Failed to send CF to peer");
                    failures.push((peer_id, error_msg));
                }
            }
        }

        if success_count == 0 && total_peers > 0 {
            return Err(BroadcastError::AllFailed(
                failures.iter().map(|(p, e)| format!("{}: {}", p, e)).collect::<Vec<_>>().join("; ")
            ));
        }

        Ok(BroadcastResult::with_failures(success_count, total_peers, failures))
    }

    async fn broadcast_vote(&self, vote: &Vote) -> Result<BroadcastResult, BroadcastError> {
        let peers = self.network.get_connected_peers();
        let total_peers = peers.len();
        
        if total_peers == 0 {
            debug!(cf_id = %vote.cf_id, "No peers to broadcast vote to");
            return Ok(BroadcastResult::success(0, 0));
        }

        let mut success_count = 0;
        let mut failures = Vec::new();

        for peer_id in peers {
            match self.network.send_vote(&peer_id, vote.clone()).await {
                Ok(_) => {
                    success_count += 1;
                    debug!(cf_id = %vote.cf_id, peer = %peer_id, "Vote sent to peer");
                }
                Err(e) => {
                    let error_msg = format!("{}", e);
                    warn!(cf_id = %vote.cf_id, peer = %peer_id, error = %error_msg, "Failed to send vote to peer");
                    failures.push((peer_id, error_msg));
                }
            }
        }

        if success_count == 0 && total_peers > 0 {
            return Err(BroadcastError::AllFailed(
                failures.iter().map(|(p, e)| format!("{}: {}", p, e)).collect::<Vec<_>>().join("; ")
            ));
        }

        Ok(BroadcastResult::with_failures(success_count, total_peers, failures))
    }

    async fn broadcast_finalized(&self, cf_id: &str) -> Result<BroadcastResult, BroadcastError> {
        // For finalization, we notify the network layer
        // The network layer may handle this differently (e.g., triggering sync)
        let peers = self.network.get_connected_peers();
        let total_peers = peers.len();
        
        if total_peers == 0 {
            debug!(cf_id = %cf_id, "No peers to broadcast finalization to");
            return Ok(BroadcastResult::success(0, 0));
        }

        // Notify state sync about finalization
        // This uses the existing notify_cf_finalized which is a local notification
        // For actual P2P broadcast, we would need to extend the network service
        // For now, we just acknowledge the operation
        
        // TODO: When AnemoNetworkService has a broadcast_finalized method, use it here
        // For MVP, finalization is implicitly communicated through vote quorum
        debug!(cf_id = %cf_id, peer_count = total_peers, "CF finalization acknowledged (implicit via votes)");
        
        Ok(BroadcastResult::success(total_peers, total_peers))
    }

    async fn broadcast_event(&self, event: &Event) -> Result<BroadcastResult, BroadcastError> {
        let total_peers = self.network.get_peer_count();
        
        if total_peers == 0 {
            debug!(event_id = %event.id, "No peers to broadcast event to");
            return Ok(BroadcastResult::success(0, 0));
        }

        // Use the network layer's broadcast_event method
        match self.network.broadcast_event(event.clone()).await {
            Ok(_) => {
                info!(
                    event_id = %event.id,
                    peer_count = total_peers,
                    "Event broadcasted to all peers"
                );
                Ok(BroadcastResult::success(total_peers, total_peers))
            }
            Err(e) => {
                let error_msg = format!("{}", e);
                warn!(event_id = %event.id, error = %error_msg, "Failed to broadcast event");
                // Network layer broadcasts to all peers, so if it fails, all failed
                Err(BroadcastError::AllFailed(error_msg))
            }
        }
    }

    async fn request_events(&self, event_ids: &[EventId]) -> Result<Vec<Event>, BroadcastError> {
        if event_ids.is_empty() {
            return Ok(Vec::new());
        }

        debug!(
            count = event_ids.len(),
            "Requesting missing events from peers"
        );

        // Convert EventId to String for network layer
        let event_id_strings: Vec<String> = event_ids.iter().cloned().collect();

        // Use the network layer's request_events_by_id method
        match self.network.request_events_by_id(&event_id_strings).await {
            Ok(events) => {
                info!(
                    requested = event_ids.len(),
                    fetched = events.len(),
                    "Successfully fetched events from peers"
                );
                Ok(events)
            }
            Err(e) => {
                warn!(error = %e, "Failed to fetch events from peers");
                Err(BroadcastError::NetworkError(format!("{}", e)))
            }
        }
    }

    fn peer_count(&self) -> usize {
        self.network.get_peer_count()
    }

    fn local_validator_id(&self) -> &str {
        &self.local_validator_id
    }
}

#[cfg(test)]
mod tests {
    // Tests would require mocking AnemoNetworkService
    // For now, integration tests should cover this functionality
}
