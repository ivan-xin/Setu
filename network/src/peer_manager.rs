// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Peer manager for managing peer connections

use crate::{
    error::{NetworkError, Result},
    peer::{Peer, PeerInfo, PeerRole},
    protocols::NetworkMessage,
};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Peer manager configuration
#[derive(Debug, Clone)]
pub struct PeerManagerConfig {
    pub max_peers: usize,
    pub max_inbound_connections: usize,
    pub max_outbound_connections: usize,
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self {
            max_peers: 100,
            max_inbound_connections: 50,
            max_outbound_connections: 50,
        }
    }
}

/// Peer manager for managing all peer connections
#[derive(Clone)]
pub struct PeerManager {
    /// Active peers indexed by peer ID
    peers: Arc<DashMap<String, Arc<Peer>>>,
    
    /// Configuration
    config: PeerManagerConfig,
}

impl PeerManager {
    /// Create a new peer manager
    pub fn new(config: PeerManagerConfig) -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            config,
        }
    }
    
    /// Add a new peer
    pub async fn add_peer(&self, peer_info: PeerInfo) -> Result<Arc<Peer>> {
        let peer_id = peer_info.node_info.id.clone();
        
        // Check if peer already exists
        if self.peers.contains_key(&peer_id) {
            return Err(NetworkError::PeerAlreadyExists(peer_id));
        }
        
        // Check max peers limit
        if self.peers.len() >= self.config.max_peers {
            return Err(NetworkError::MaxPeersReached(self.config.max_peers));
        }
        
        // Create channels for peer communication
        let (outbound_tx, _outbound_rx) = mpsc::channel(1024);
        let (_inbound_tx, inbound_rx) = mpsc::channel(1024);
        
        // Create peer
        let peer = Arc::new(Peer::new(peer_info, outbound_tx, inbound_rx));
        
        // Insert peer
        self.peers.insert(peer_id.clone(), peer.clone());
        
        Ok(peer)
    }
    
    /// Remove a peer
    pub async fn remove_peer(&self, peer_id: &str) -> Result<()> {
        if let Some((_, peer)) = self.peers.remove(peer_id) {
            peer.mark_disconnected().await;
            Ok(())
        } else {
            Err(NetworkError::PeerNotFound(peer_id.to_string()))
        }
    }
    
    /// Get a peer by ID
    pub fn get_peer(&self, peer_id: &str) -> Option<Arc<Peer>> {
        self.peers.get(peer_id).map(|entry| entry.value().clone())
    }
    
    /// Get all peer IDs
    pub fn get_all_peer_ids(&self) -> Vec<String> {
        self.peers.iter().map(|entry| entry.key().clone()).collect()
    }
    
    /// Get all connected peers
    pub async fn get_connected_peers(&self) -> Vec<Arc<Peer>> {
        let mut peers = Vec::new();
        for entry in self.peers.iter() {
            let peer = entry.value();
            if peer.is_connected().await {
                peers.push(peer.clone());
            }
        }
        peers
    }
    
    /// Get validators
    pub async fn get_validators(&self) -> Vec<PeerInfo> {
        let mut validators = Vec::new();
        for entry in self.peers.iter() {
            let peer_info = entry.value().info().await;
            if peer_info.role == PeerRole::Validator && peer_info.is_connected() {
                validators.push(peer_info);
            }
        }
        validators
    }
    
    /// Get solvers
    pub async fn get_solvers(&self) -> Vec<PeerInfo> {
        let mut solvers = Vec::new();
        for entry in self.peers.iter() {
            let peer_info = entry.value().info().await;
            if peer_info.role == PeerRole::Solver && peer_info.is_connected() {
                solvers.push(peer_info);
            }
        }
        solvers
    }
    
    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
    
    /// Get connected peer count
    pub async fn connected_peer_count(&self) -> usize {
        let mut count = 0;
        for entry in self.peers.iter() {
            if entry.value().is_connected().await {
                count += 1;
            }
        }
        count
    }
    
    /// Update peer's last seen time
    pub async fn update_peer_last_seen(&self, peer_id: &str) {
        if let Some(peer) = self.get_peer(peer_id) {
            peer.update_last_seen().await;
        }
    }
    
    /// Broadcast message to all connected peers
    pub async fn broadcast(&self, message: NetworkMessage) -> Result<()> {
        let peers = self.get_connected_peers().await;
        for peer in peers {
            let _ = peer.send(message.clone()).await; // Ignore individual errors
        }
        Ok(())
    }
    
    /// Send message to specific peer
    pub async fn send_to_peer(&self, peer_id: &str, message: NetworkMessage) -> Result<()> {
        let peer = self
            .get_peer(peer_id)
            .ok_or_else(|| NetworkError::PeerNotFound(peer_id.to_string()))?;
        peer.send(message).await
    }
    
    /// Send message to all validators
    pub async fn send_to_validators(&self, message: NetworkMessage) -> Result<()> {
        let validators = self.get_validators().await;
        for validator in validators {
            let _ = self.send_to_peer(&validator.node_info.id, message.clone()).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use setu_types::NodeInfo;
    
    fn create_test_node_info(id: &str) -> NodeInfo {
        NodeInfo::new_validator(id.to_string(), "127.0.0.1".to_string(), 9000)
    }
    
    #[tokio::test]
    async fn test_peer_manager_add_remove() {
        let config = PeerManagerConfig::default();
        let manager = PeerManager::new(config);
        
        let node_info = create_test_node_info("test-peer");
        let peer_info = PeerInfo::new(node_info);
        
        // Add peer
        let result = manager.add_peer(peer_info.clone()).await;
        assert!(result.is_ok());
        assert_eq!(manager.peer_count(), 1);
        
        // Try to add same peer again
        let result = manager.add_peer(peer_info).await;
        assert!(result.is_err());
        
        // Remove peer
        let result = manager.remove_peer("test-peer").await;
        assert!(result.is_ok());
        assert_eq!(manager.peer_count(), 0);
    }
}
