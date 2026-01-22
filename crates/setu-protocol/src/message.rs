// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network message definitions for Setu protocol
//!
//! This module defines the core `SetuMessage` enum that represents all
//! possible messages exchanged between Setu nodes.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use setu_types::{ConsensusFrame, Event, Vote};

/// Network messages for Setu protocol
///
/// This enum defines all possible message types that can be sent between nodes.
/// Each variant carries the data needed for that specific operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SetuMessage {
    /// Event broadcast - propagate a new event to peers
    EventBroadcast {
        event: Event,
        sender_id: String,
    },

    /// Consensus frame proposal - propose a new CF for voting
    CFProposal {
        cf: ConsensusFrame,
        proposer_id: String,
    },

    /// Vote for a consensus frame
    CFVote {
        vote: Vote,
    },

    /// Notification that a consensus frame has been finalized
    CFFinalized {
        cf: ConsensusFrame,
        /// ID of the node that sent this notification
        sender_id: String,
    },

    /// Request specific events by their IDs
    RequestEvents {
        event_ids: Vec<String>,
        requester_id: String,
    },

    /// Response containing requested events
    EventsResponse {
        events: Vec<Event>,
        responder_id: String,
    },

    /// Ping message for health check and latency measurement
    Ping {
        timestamp: u64,
        nonce: u64,
    },

    /// Pong response to a ping
    Pong {
        timestamp: u64,
        nonce: u64,
    },
}

/// Message type identifier
///
/// Used for routing and filtering messages without deserializing the full payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    EventBroadcast,
    CFProposal,
    CFVote,
    CFFinalized,
    RequestEvents,
    EventsResponse,
    Ping,
    Pong,
}

impl SetuMessage {
    /// Get the message type without accessing the payload
    pub fn message_type(&self) -> MessageType {
        match self {
            SetuMessage::EventBroadcast { .. } => MessageType::EventBroadcast,
            SetuMessage::CFProposal { .. } => MessageType::CFProposal,
            SetuMessage::CFVote { .. } => MessageType::CFVote,
            SetuMessage::CFFinalized { .. } => MessageType::CFFinalized,
            SetuMessage::RequestEvents { .. } => MessageType::RequestEvents,
            SetuMessage::EventsResponse { .. } => MessageType::EventsResponse,
            SetuMessage::Ping { .. } => MessageType::Ping,
            SetuMessage::Pong { .. } => MessageType::Pong,
        }
    }

    /// Check if this message type expects a response
    pub fn expects_response(&self) -> bool {
        matches!(
            self,
            SetuMessage::RequestEvents { .. } | SetuMessage::Ping { .. }
        )
    }

    /// Check if this is a consensus-related message
    pub fn is_consensus_message(&self) -> bool {
        matches!(
            self,
            SetuMessage::CFProposal { .. }
                | SetuMessage::CFVote { .. }
                | SetuMessage::CFFinalized { .. }
        )
    }
}

/// Message codec for serialization/deserialization
pub struct MessageCodec;

impl MessageCodec {
    /// Encode a message to bytes
    pub fn encode(message: &SetuMessage) -> Result<Bytes, MessageCodecError> {
        let bytes = bincode::serialize(message)
            .map_err(|e| MessageCodecError::SerializationError(e.to_string()))?;
        Ok(Bytes::from(bytes))
    }

    /// Decode a message from bytes
    pub fn decode(bytes: &[u8]) -> Result<SetuMessage, MessageCodecError> {
        bincode::deserialize(bytes)
            .map_err(|e| MessageCodecError::DeserializationError(e.to_string()))
    }
}

/// Errors that can occur during message encoding/decoding
#[derive(Debug, thiserror::Error)]
pub enum MessageCodecError {
    #[error("Failed to serialize message: {0}")]
    SerializationError(String),

    #[error("Failed to deserialize message: {0}")]
    DeserializationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use setu_types::VLCSnapshot;

    #[test]
    fn test_message_roundtrip() {
        let event = Event::genesis("test_creator".to_string(), VLCSnapshot::default());
        let msg = SetuMessage::EventBroadcast {
            event,
            sender_id: "sender1".to_string(),
        };

        let encoded = MessageCodec::encode(&msg).unwrap();
        let decoded = MessageCodec::decode(&encoded).unwrap();

        assert!(matches!(decoded, SetuMessage::EventBroadcast { .. }));
    }

    #[test]
    fn test_message_type() {
        let msg = SetuMessage::Ping {
            timestamp: 123,
            nonce: 456,
        };
        assert_eq!(msg.message_type(), MessageType::Ping);
        assert!(msg.expects_response());
        assert!(!msg.is_consensus_message());
    }

    #[test]
    fn test_consensus_message_check() {
        let vote = Vote::new(
            "v1".to_string(),
            "cf1".to_string(),
            true,
        );
        let msg = SetuMessage::CFVote { vote };
        assert!(msg.is_consensus_message());
    }
}
