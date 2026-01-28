// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! RPC request/response types for state synchronization
//!
//! This module defines the data structures used in state sync RPC calls.
//! These types are specific to the VLC + ConsensusFrame consensus and
//! handle synchronization of Events and CFs between validators.
//!
//! ## Route Paths
//!
//! - `/setu/sync/events` - Event synchronization
//! - `/setu/sync/cfs` - ConsensusFrame synchronization
//! - `/setu/sync/state` - Sync state query

use serde::{Deserialize, Serialize};

// ============================================================================
// Serialized Types for Network Transfer
// ============================================================================

/// A serialized event for network transfer
///
/// Events are serialized to bytes for efficient network transmission and
/// storage. The `data` field contains the bincode-serialized `Event`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedEvent {
    /// Sequence number (for ordering)
    pub seq: u64,
    /// Event ID (for deduplication and lookup)
    pub id: String,
    /// Serialized event data (bincode)
    pub data: Vec<u8>,
}

/// A serialized consensus frame for network transfer
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedConsensusFrame {
    /// CF sequence number
    pub seq: u64,
    /// CF ID
    pub id: String,
    /// Serialized CF data (bincode)
    pub data: Vec<u8>,
    /// Optional VLC state at this CF (serialized)
    pub vlc: Option<Vec<u8>>,
}

/// A serialized vote for network transfer
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerializedVote {
    /// Voter ID (validator)
    pub voter_id: String,
    /// ID of the CF being voted on
    pub cf_id: String,
    /// Signature bytes
    pub signature: Vec<u8>,
}

// ============================================================================
// Event Sync Request/Response
// ============================================================================

/// Request for synchronizing events
///
/// Route: `/setu/sync/events`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncEventsRequest {
    /// Start from this sequence number (exclusive)
    pub start_seq: u64,
    /// Maximum number of events to return
    pub limit: u32,
}

/// Response containing events
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncEventsResponse {
    /// Serialized events
    pub events: Vec<SerializedEvent>,
    /// Whether there are more events after this batch
    pub has_more: bool,
    /// The highest sequence number included
    pub highest_seq: u64,
}

// ============================================================================
// ConsensusFrame Sync Request/Response
// ============================================================================

/// Request for synchronizing consensus frames
///
/// Route: `/setu/sync/cfs`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncConsensusFramesRequest {
    /// Start from this CF sequence (exclusive)
    pub start_seq: u64,
    /// Maximum number of CFs to return
    pub limit: u32,
}

/// Response containing consensus frames
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncConsensusFramesResponse {
    /// Serialized consensus frames
    pub frames: Vec<SerializedConsensusFrame>,
    /// Whether there are more CFs after this batch
    pub has_more: bool,
    /// The highest CF sequence included
    pub highest_seq: u64,
}

// ============================================================================
// Push Request/Response (for proactive sync)
// ============================================================================

/// Request for pushing events to a peer
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushEventsRequest {
    /// Events to push
    pub events: Vec<SerializedEvent>,
}

/// Response for pushed events
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushEventsResponse {
    /// Number of events accepted
    pub accepted: u32,
    /// IDs of events that were rejected
    pub rejected: Vec<String>,
}

/// Request for pushing a consensus frame
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushConsensusFrameRequest {
    /// The consensus frame
    pub frame: SerializedConsensusFrame,
    /// Votes for this frame
    pub votes: Vec<SerializedVote>,
}

/// Response for pushed consensus frame
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushConsensusFrameResponse {
    /// Whether the CF was accepted
    pub accepted: bool,
    /// Reason if rejected
    pub reason: Option<String>,
}

// ============================================================================
// Sync State Query
// ============================================================================

/// Information about a peer's sync state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerSyncInfo {
    /// Highest event sequence the peer has
    pub highest_event_seq: u64,
    /// Highest finalized CF sequence the peer has
    pub highest_cf_seq: u64,
    /// Timestamp of last update
    pub last_update: u64,
}

/// Request for getting a peer's sync state
///
/// Route: `/setu/sync/state`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetSyncStateRequest {}

/// Response with peer's sync state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetSyncStateResponse {
    /// This node's sync info
    pub sync_info: PeerSyncInfo,
}

// ============================================================================
// Route Path Constants
// ============================================================================

/// Route paths for sync protocol
pub mod routes {
    /// Route for event sync requests
    pub const SYNC_EVENTS: &str = "/setu/sync/events";
    /// Route for CF sync requests
    pub const SYNC_CFS: &str = "/setu/sync/cfs";
    /// Route for sync state query
    pub const SYNC_STATE: &str = "/setu/sync/state";
    /// Route for pushing events
    pub const PUSH_EVENTS: &str = "/setu/push/events";
    /// Route for pushing CFs
    pub const PUSH_CF: &str = "/setu/push/cf";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_events_request_serialization() {
        let req = SyncEventsRequest {
            start_seq: 100,
            limit: 50,
        };
        let bytes = bincode::serialize(&req).unwrap();
        let decoded: SyncEventsRequest = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.start_seq, 100);
        assert_eq!(decoded.limit, 50);
    }

    #[test]
    fn test_peer_sync_info() {
        let info = PeerSyncInfo {
            highest_event_seq: 1000,
            highest_cf_seq: 50,
            last_update: 1234567890,
        };
        let bytes = bincode::serialize(&info).unwrap();
        let decoded: PeerSyncInfo = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.highest_event_seq, 1000);
        assert_eq!(decoded.highest_cf_seq, 50);
    }
}
