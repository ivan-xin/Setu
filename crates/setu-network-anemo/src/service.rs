// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network service implementation using Anemo
//!
//! This module provides the main network service that integrates:
//! - Discovery: Peer discovery and connection management
//! - StateSync: Event and ConsensusFrame synchronization
//! - Transport: QUIC-based communication
//!
//! The service follows Sui's patterns using the builder pattern for construction.

use crate::{
    config::NetworkConfig,
    discovery::{self, DiscoveryConfig, NodeType, SignedNodeInfo},
    error::Result,
    metrics::NetworkMetrics,
    peer_manager::AnemoPeerManager,
    state_sync::{self, StateSyncConfig},
    transport::AnemoTransport,
    AnemoError,
};
use anemo::PeerId;
use bytes::Bytes;
use setu_types::{ConsensusFrame, Event, NodeInfo, Vote};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Network events that can be sent to application
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

    /// Received an event broadcast
    EventReceived {
        peer_id: String,
        event: Event,
    },

    /// Received a consensus frame proposal
    CFProposal {
        peer_id: String,
        cf: ConsensusFrame,
    },

    /// Received a vote
    VoteReceived {
        peer_id: String,
        vote: Vote,
    },

    /// Received a CF finalized notification
    CFFinalized {
        peer_id: String,
        cf: ConsensusFrame,
    },
}

/// Network messages for Setu protocol
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SetuMessage {
    /// Event broadcast
    EventBroadcast { event: Event, sender_id: String },

    /// Consensus frame proposal
    CFProposal { cf: ConsensusFrame, proposer_id: String },

    /// Vote for a consensus frame
    CFVote { vote: Vote },

    /// Consensus frame finalized
    CFFinalized { cf: ConsensusFrame },

    /// Ping message for health check
    Ping { timestamp: u64, nonce: u64 },

    /// Pong response
    Pong { timestamp: u64, nonce: u64 },
}

/// Anemo-based network service for Setu
///
/// This is the main entry point for network functionality. It integrates
/// discovery, state sync, and transport into a unified service.
pub struct AnemoNetworkService {
    /// Anemo transport
    transport: Arc<AnemoTransport>,

    /// Peer manager
    peer_manager: Arc<AnemoPeerManager>,

    /// Discovery handle (for controlling discovery)
    discovery_handle: Option<discovery::Handle>,

    /// State sync handle (for controlling sync)
    state_sync_handle: Option<state_sync::Handle>,

    /// Local node information
    local_node_info: NodeInfo,

    /// Node type (validator or solver)
    node_type: NodeType,

    /// Event sender for application
    event_tx: mpsc::Sender<NetworkEvent>,

    /// Metrics (optional)
    #[allow(dead_code)]
    metrics: Option<NetworkMetrics>,
}

impl AnemoNetworkService {
    /// Create a new Anemo network service
    pub async fn new(
        config: NetworkConfig,
        local_node_info: NodeInfo,
        event_tx: mpsc::Sender<NetworkEvent>,
    ) -> Result<Self> {
        info!("Creating Anemo network service for node {}", local_node_info.id);

        // Create transport
        let transport = Arc::new(AnemoTransport::new(&config.anemo).await?);

        // Create peer manager
        let peer_manager = Arc::new(AnemoPeerManager::new(transport.clone())?);

        Ok(Self {
            transport,
            peer_manager,
            discovery_handle: None,
            state_sync_handle: None,
            local_node_info,
            node_type: NodeType::Validator, // Default, can be configured
            event_tx,
            metrics: None,
        })
    }

    /// Create with metrics
    pub async fn with_metrics(
        config: NetworkConfig,
        local_node_info: NodeInfo,
        event_tx: mpsc::Sender<NetworkEvent>,
        registry: &prometheus::Registry,
    ) -> Result<Self> {
        let mut service = Self::new(config, local_node_info, event_tx).await?;
        service.metrics = Some(NetworkMetrics::new(registry));
        Ok(service)
    }

    /// Set the node type
    pub fn set_node_type(&mut self, node_type: NodeType) {
        self.node_type = node_type;
    }

    /// Start the network service with discovery and state sync
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting Anemo network service for node {}", self.local_node_info.id);
        
        let network = self.transport.network();
        let network_ref = network.downgrade();

        // Build and start discovery
        let discovery_config = DiscoveryConfig::default();
        let (unstarted_discovery, _discovery_server) = discovery::Builder::new()
            .config(discovery_config)
            .build();
        
        let (discovery_handle, _join_handle) = unstarted_discovery.start(network_ref.clone());
        self.discovery_handle = Some(discovery_handle);

        // Build and start state sync (with a dummy store for now)
        let state_sync_config = StateSyncConfig::default();
        let (unstarted_state_sync, _state_sync_server) = state_sync::Builder::new()
            .store(DummyStore)
            .config(state_sync_config)
            .build();
        
        let (state_sync_handle, _join_handle) = unstarted_state_sync.start(network_ref);
        self.state_sync_handle = Some(state_sync_handle);
        
        info!("Anemo network service is ready");
        Ok(())
    }

    /// Get local peer ID
    pub fn peer_id(&self) -> PeerId {
        self.transport.peer_id()
    }

    /// Get local address
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.transport.local_addr()
    }

    /// Connect to a peer
    pub async fn connect_to_peer(&self, node_info: NodeInfo) -> Result<PeerId> {
        let addr = format!("{}:{}", node_info.address, node_info.port);
        let socket_addr: std::net::SocketAddr = addr
            .parse()
            .map_err(|e| AnemoError::InvalidConfig(format!("Invalid address: {}", e)))?;
        let peer_id = self.transport.connect(socket_addr).await?;
        
        // Add to peer manager
        self.peer_manager.add_peer(node_info.clone(), peer_id).await?;

        // Notify state sync
        if let Some(ref handle) = self.state_sync_handle {
            handle.notify_new_event(0); // Will trigger sync with new peer
        }

        Ok(peer_id)
    }

    /// Disconnect from a peer
    pub async fn disconnect_from_peer(&self, peer_id_str: &str) -> Result<()> {
        let peer_id = self.parse_peer_id(peer_id_str)?;
        self.peer_manager.disconnect_from_peer(&peer_id).await?;
        Ok(())
    }

    /// Broadcast an event to all connected peers
    pub async fn broadcast_event(&self, event: Event) -> Result<()> {
        debug!("Broadcasting event: {:?}", event.id);

        let message = SetuMessage::EventBroadcast {
            event,
            sender_id: self.local_node_info.id.clone(),
        };

        let bytes = self.serialize_message(&message)?;
        let peers = self.peer_manager.get_connected_peers();

        for peer_info in peers {
            let request = anemo::Request::new(bytes.clone());
            if let Err(e) = self.transport.rpc(peer_info.peer_id, request).await {
                tracing::warn!("Failed to send event to peer {}: {}", peer_info.peer_id, e);
            }
        }

        Ok(())
    }

    /// Send a consensus frame proposal
    pub async fn send_cf_proposal(&self, peer_id_str: &str, cf: ConsensusFrame) -> Result<()> {
        debug!("Sending CF proposal to peer {}", peer_id_str);

        let peer_id = self.parse_peer_id(peer_id_str)?;
        let message = SetuMessage::CFProposal {
            cf,
            proposer_id: self.local_node_info.id.clone(),
        };

        let bytes = self.serialize_message(&message)?;
        let request = anemo::Request::new(bytes);
        self.transport.rpc(peer_id, request).await?;

        Ok(())
    }

    /// Send a vote
    pub async fn send_vote(&self, peer_id_str: &str, vote: Vote) -> Result<()> {
        debug!("Sending vote to peer {}", peer_id_str);

        let peer_id = self.parse_peer_id(peer_id_str)?;
        let message = SetuMessage::CFVote { vote };

        let bytes = self.serialize_message(&message)?;
        let request = anemo::Request::new(bytes);
        self.transport.rpc(peer_id, request).await?;

        Ok(())
    }

    /// Get connected peer count
    pub fn get_peer_count(&self) -> usize {
        self.peer_manager.get_connected_peers().len()
    }

    /// Get all connected peers
    pub fn get_connected_peers(&self) -> Vec<String> {
        self.peer_manager
            .get_connected_peers()
            .into_iter()
            .map(|p| format!("{}", p.peer_id))
            .collect()
    }

    /// Notify that a new event was created locally
    pub fn notify_new_event(&self, event_seq: u64) {
        if let Some(ref handle) = self.state_sync_handle {
            handle.notify_new_event(event_seq);
        }
    }

    /// Notify that a CF was finalized
    pub fn notify_cf_finalized(&self, cf_seq: u64) {
        if let Some(ref handle) = self.state_sync_handle {
            handle.notify_cf_finalized(cf_seq);
        }
    }

    /// Request sync with a specific peer
    pub fn request_sync_with_peer(&self, peer_id: PeerId) {
        if let Some(ref handle) = self.state_sync_handle {
            handle.sync_with_peer(peer_id);
        }
    }

    /// Shutdown the network service
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down network service");
        
        // Handles are dropped, which signals shutdown
        // Transport shutdown
        self.transport.shutdown().await?;
        
        info!("Network service shutdown complete");
        Ok(())
    }

    /// Helper: Parse peer ID from string
    fn parse_peer_id(&self, peer_id_str: &str) -> Result<PeerId> {
        let bytes = hex::decode(peer_id_str)
            .map_err(|e| AnemoError::InvalidConfig(format!("Invalid peer ID: {}", e)))?;

        if bytes.len() != 32 {
            return Err(AnemoError::InvalidConfig(
                "Peer ID must be 32 bytes".to_string(),
            ));
        }

        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(PeerId(array))
    }

    /// Helper: Serialize message
    fn serialize_message(&self, message: &SetuMessage) -> Result<Bytes> {
        let bytes = bincode::serialize(message)?;
        Ok(Bytes::from(bytes))
    }

    /// Helper: Deserialize message
    #[allow(dead_code)]
    fn deserialize_message(&self, bytes: &[u8]) -> Result<SetuMessage> {
        let message = bincode::deserialize(bytes)?;
        Ok(message)
    }
}

/// Dummy store implementation for state sync
/// TODO: Replace with actual storage interface
#[derive(Clone)]
struct DummyStore;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AnemoConfig;

    #[tokio::test]
    async fn test_network_service_creation() {
        let config = NetworkConfig {
            anemo: AnemoConfig {
                listen_addr: "127.0.0.1:0".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let node_info = NodeInfo::new_validator(
            "test-node".to_string(),
            "127.0.0.1".to_string(),
            9000,
        );

        let (event_tx, _event_rx) = mpsc::channel(100);

        let service = AnemoNetworkService::new(config, node_info, event_tx)
            .await
            .unwrap();

        assert_eq!(service.get_peer_count(), 0);
    }
}
