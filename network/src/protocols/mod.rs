// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network protocols and message types

use serde::{Deserialize, Serialize};
use setu_types::{ConsensusFrame, Event, Vote};

pub mod direct_send;
pub mod rpc;
pub mod wire;

pub use direct_send::DirectSendMsg;
pub use rpc::{RpcRequest, RpcResponse};

/// Protocol identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ProtocolId {
    /// Consensus protocol
    Consensus = 0,
    /// Event broadcast protocol
    EventBroadcast = 1,
    /// State sync protocol
    StateSync = 2,
    /// Health check protocol
    HealthCheck = 3,
    /// Discovery protocol
    Discovery = 4,
}

impl ProtocolId {
    /// Convert to byte
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
    
    /// Convert from byte
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(ProtocolId::Consensus),
            1 => Some(ProtocolId::EventBroadcast),
            2 => Some(ProtocolId::StateSync),
            3 => Some(ProtocolId::HealthCheck),
            4 => Some(ProtocolId::Discovery),
            _ => None,
        }
    }
}

/// Network message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Direct send message
    DirectSend(DirectSendMsg),
    
    /// RPC request
    RpcRequest(RpcRequest),
    
    /// RPC response
    RpcResponse(RpcResponse),
    
    /// Event broadcast
    EventBroadcast {
        event: Event,
        sender_id: String,
    },
    
    /// Consensus frame proposal
    CFProposal {
        cf: ConsensusFrame,
        proposer_id: String,
    },
    
    /// Consensus frame vote
    CFVote {
        vote: Vote,
    },
    
    /// Consensus frame finalized
    CFFinalized {
        cf: ConsensusFrame,
    },
    
    /// Ping message
    Ping {
        timestamp: u64,
        nonce: u64,
    },
    
    /// Pong message
    Pong {
        timestamp: u64,
        nonce: u64,
    },
    
    /// Peer discovery request
    PeerDiscovery {
        peers: Vec<String>,
    },
    
    /// Disconnect message
    Disconnect {
        reason: String,
    },
}

impl NetworkMessage {
    /// Get the protocol ID for this message
    pub fn protocol_id(&self) -> ProtocolId {
        match self {
            NetworkMessage::CFProposal { .. }
            | NetworkMessage::CFVote { .. }
            | NetworkMessage::CFFinalized { .. } => ProtocolId::Consensus,
            NetworkMessage::EventBroadcast { .. } => ProtocolId::EventBroadcast,
            NetworkMessage::Ping { .. } | NetworkMessage::Pong { .. } => ProtocolId::HealthCheck,
            NetworkMessage::PeerDiscovery { .. } => ProtocolId::Discovery,
            NetworkMessage::DirectSend(msg) => msg.protocol_id,
            NetworkMessage::RpcRequest(req) => req.protocol_id,
            NetworkMessage::RpcResponse(resp) => resp.protocol_id,
            NetworkMessage::Disconnect { .. } => ProtocolId::HealthCheck,
        }
    }
    
    /// Get the size of the message payload
    pub fn payload_size(&self) -> usize {
        bincode::serialize(self).map(|bytes| bytes.len()).unwrap_or(0)
    }
    
    /// Create an event broadcast message
    pub fn event_broadcast(event: Event, sender_id: String) -> Self {
        NetworkMessage::EventBroadcast { event, sender_id }
    }
    
    /// Create a CF proposal message
    pub fn cf_proposal(cf: ConsensusFrame, proposer_id: String) -> Self {
        NetworkMessage::CFProposal { cf, proposer_id }
    }
    
    /// Create a CF vote message
    pub fn cf_vote(vote: Vote) -> Self {
        NetworkMessage::CFVote { vote }
    }
    
    /// Create a CF finalized message
    pub fn cf_finalized(cf: ConsensusFrame) -> Self {
        NetworkMessage::CFFinalized { cf }
    }
    
    /// Create a ping message
    pub fn ping(timestamp: u64, nonce: u64) -> Self {
        NetworkMessage::Ping { timestamp, nonce }
    }
    
    /// Create a pong message
    pub fn pong(timestamp: u64, nonce: u64) -> Self {
        NetworkMessage::Pong { timestamp, nonce }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_protocol_id_conversion() {
        assert_eq!(ProtocolId::Consensus.as_u8(), 0);
        assert_eq!(ProtocolId::from_u8(0), Some(ProtocolId::Consensus));
        assert_eq!(ProtocolId::from_u8(255), None);
    }
    
    #[test]
    fn test_message_protocol_id() {
        let ping = NetworkMessage::ping(123, 456);
        assert_eq!(ping.protocol_id(), ProtocolId::HealthCheck);
    }
}
