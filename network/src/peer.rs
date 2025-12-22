// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Peer management

use crate::error::{NetworkError, Result};
use crate::protocols::NetworkMessage;
use setu_types::NodeInfo;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};

/// Peer role in the network
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerRole {
    /// Validator node
    Validator,
    /// Solver node
    Solver,
    /// Unknown role
    Unknown,
}

impl std::fmt::Display for PeerRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerRole::Validator => write!(f, "Validator"),
            PeerRole::Solver => write!(f, "Solver"),
            PeerRole::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Peer connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    /// Attempting to connect
    Connecting,
    /// Successfully connected
    Connected,
    /// Disconnecting
    Disconnecting,
    /// Disconnected
    Disconnected,
}

/// Information about a peer
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Node information
    pub node_info: NodeInfo,
    
    /// Peer role
    pub role: PeerRole,
    
    /// Current connection state
    pub state: PeerState,
    
    /// Last seen timestamp
    pub last_seen: SystemTime,
    
    /// Connection timestamp
    pub connected_at: Option<SystemTime>,
    
    /// Number of connection failures
    pub failure_count: usize,
    
    /// Whether the connection is inbound
    pub is_inbound: bool,
}

impl PeerInfo {
    /// Create new peer info
    pub fn new(node_info: NodeInfo) -> Self {
        Self {
            node_info,
            role: PeerRole::Unknown,
            state: PeerState::Disconnected,
            last_seen: SystemTime::now(),
            connected_at: None,
            failure_count: 0,
            is_inbound: false,
        }
    }
    
    /// Create new peer info with role
    pub fn with_role(node_info: NodeInfo, role: PeerRole) -> Self {
        Self {
            node_info,
            role,
            state: PeerState::Disconnected,
            last_seen: SystemTime::now(),
            connected_at: None,
            failure_count: 0,
            is_inbound: false,
        }
    }
    
    /// Mark peer as connected
    pub fn mark_connected(&mut self) {
        self.state = PeerState::Connected;
        self.connected_at = Some(SystemTime::now());
        self.last_seen = SystemTime::now();
        self.failure_count = 0;
    }
    
    /// Mark peer as disconnected
    pub fn mark_disconnected(&mut self) {
        self.state = PeerState::Disconnected;
        self.connected_at = None;
    }
    
    /// Update last seen time
    pub fn update_last_seen(&mut self) {
        self.last_seen = SystemTime::now();
    }
    
    /// Increment failure count
    pub fn increment_failure_count(&mut self) {
        self.failure_count += 1;
    }
    
    /// Check if peer is connected
    pub fn is_connected(&self) -> bool {
        self.state == PeerState::Connected
    }
    
    /// Get connection duration
    pub fn connection_duration(&self) -> Option<Duration> {
        self.connected_at
            .and_then(|connected_at| SystemTime::now().duration_since(connected_at).ok())
    }
    
    /// Get idle duration (time since last seen)
    pub fn idle_duration(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.last_seen)
            .unwrap_or(Duration::ZERO)
    }
}

/// Peer connection handler
pub struct Peer {
    /// Peer information
    info: Arc<RwLock<PeerInfo>>,
    
    /// Channel for sending messages to peer
    outbound_tx: mpsc::Sender<NetworkMessage>,
    
    /// Channel for receiving messages from peer
    inbound_rx: Arc<RwLock<mpsc::Receiver<NetworkMessage>>>,
}

impl Peer {
    /// Create a new peer
    pub fn new(
        info: PeerInfo,
        outbound_tx: mpsc::Sender<NetworkMessage>,
        inbound_rx: mpsc::Receiver<NetworkMessage>,
    ) -> Self {
        Self {
            info: Arc::new(RwLock::new(info)),
            outbound_tx,
            inbound_rx: Arc::new(RwLock::new(inbound_rx)),
        }
    }
    
    /// Get peer info
    pub async fn info(&self) -> PeerInfo {
        self.info.read().await.clone()
    }
    
    /// Get peer ID
    pub async fn peer_id(&self) -> String {
        self.info.read().await.node_info.id.clone()
    }
    
    /// Send a message to the peer
    pub async fn send(&self, message: NetworkMessage) -> Result<()> {
        self.outbound_tx
            .send(message)
            .await
            .map_err(|_| NetworkError::ChannelSendError)?;
        Ok(())
    }
    
    /// Receive a message from the peer
    pub async fn receive(&self) -> Option<NetworkMessage> {
        self.inbound_rx.write().await.recv().await
    }
    
    /// Mark peer as connected
    pub async fn mark_connected(&self) {
        self.info.write().await.mark_connected();
    }
    
    /// Mark peer as disconnected
    pub async fn mark_disconnected(&self) {
        self.info.write().await.mark_disconnected();
    }
    
    /// Update last seen time
    pub async fn update_last_seen(&self) {
        self.info.write().await.update_last_seen();
    }
    
    /// Check if peer is connected
    pub async fn is_connected(&self) -> bool {
        self.info.read().await.is_connected()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_node_info() -> NodeInfo {
        NodeInfo::new_validator("test-node".to_string(), "127.0.0.1".to_string(), 9000)
    }
    
    #[test]
    fn test_peer_info_creation() {
        let node_info = create_test_node_info();
        
        let peer_info = PeerInfo::new(node_info);
        assert_eq!(peer_info.role, PeerRole::Unknown);
        assert_eq!(peer_info.state, PeerState::Disconnected);
        assert!(!peer_info.is_connected());
    }
    
    #[test]
    fn test_peer_state_transitions() {
        let node_info = create_test_node_info();
        
        let mut peer_info = PeerInfo::new(node_info);
        assert!(!peer_info.is_connected());
        
        peer_info.mark_connected();
        assert!(peer_info.is_connected());
        assert!(peer_info.connected_at.is_some());
        
        peer_info.mark_disconnected();
        assert!(!peer_info.is_connected());
        assert!(peer_info.connected_at.is_none());
    }
}
