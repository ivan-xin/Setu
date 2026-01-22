// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network events for application layer notification
//!
//! This module defines events that the network layer sends to the application
//! layer (e.g., ConsensusEngine) when network activities occur.

use setu_types::{ConsensusFrame, Event, NodeInfo, Vote};

/// Network events that are sent to the application layer
///
/// These events notify the application about network activities such as
/// peer connections, received messages, and state changes.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// A new peer has connected
    PeerConnected {
        peer_id: String,
        node_info: NodeInfo,
    },

    /// A peer has disconnected
    PeerDisconnected {
        peer_id: String,
    },

    /// Received an event broadcast from a peer
    EventReceived {
        peer_id: String,
        event: Event,
    },

    /// Received a consensus frame proposal
    CFProposal {
        peer_id: String,
        cf: ConsensusFrame,
    },

    /// Received a vote for a consensus frame
    VoteReceived {
        peer_id: String,
        vote: Vote,
    },

    /// Received notification that a CF was finalized
    CFFinalized {
        peer_id: String,
        cf: ConsensusFrame,
    },
}

impl NetworkEvent {
    /// Get the peer ID associated with this event
    pub fn peer_id(&self) -> &str {
        match self {
            NetworkEvent::PeerConnected { peer_id, .. }
            | NetworkEvent::PeerDisconnected { peer_id }
            | NetworkEvent::EventReceived { peer_id, .. }
            | NetworkEvent::CFProposal { peer_id, .. }
            | NetworkEvent::VoteReceived { peer_id, .. }
            | NetworkEvent::CFFinalized { peer_id, .. } => peer_id,
        }
    }

    /// Check if this is a connection-related event
    pub fn is_connection_event(&self) -> bool {
        matches!(
            self,
            NetworkEvent::PeerConnected { .. } | NetworkEvent::PeerDisconnected { .. }
        )
    }

    /// Check if this is a consensus-related event
    pub fn is_consensus_event(&self) -> bool {
        matches!(
            self,
            NetworkEvent::CFProposal { .. }
                | NetworkEvent::VoteReceived { .. }
                | NetworkEvent::CFFinalized { .. }
        )
    }
}
