// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network interface for application protocols

use crate::error::Result;
use crate::protocols::NetworkMessage;
use async_trait::async_trait;
use setu_types::{ConsensusFrame, Event, Vote};

/// Network interface for applications
#[async_trait]
pub trait NetworkInterface: Send + Sync {
    /// Broadcast an event to all peers
    async fn broadcast_event(&self, event: Event) -> Result<()>;
    
    /// Broadcast a consensus frame proposal
    async fn broadcast_cf_proposal(&self, cf: ConsensusFrame, proposer_id: String) -> Result<()>;
    
    /// Broadcast a vote
    async fn broadcast_vote(&self, vote: Vote) -> Result<()>;
    
    /// Broadcast a finalized consensus frame
    async fn broadcast_cf_finalized(&self, cf: ConsensusFrame) -> Result<()>;
    
    /// Send a message to a specific peer
    async fn send_to_peer(&self, peer_id: &str, message: NetworkMessage) -> Result<()>;
    
    /// Send a message to all validators
    async fn send_to_validators(&self, message: NetworkMessage) -> Result<()>;
    
    /// Get the number of connected peers
    async fn get_peer_count(&self) -> usize;
    
    /// Get the list of connected validator IDs
    async fn get_validator_ids(&self) -> Vec<String>;
    
    /// Get the list of connected solver IDs
    async fn get_solver_ids(&self) -> Vec<String>;
}
