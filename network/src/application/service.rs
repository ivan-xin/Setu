// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network service implementation

use crate::{
    application::interface::NetworkInterface,
    connectivity_manager::ConnectivityRequest,
    error::Result,
    peer::{PeerInfo, PeerRole},
    peer_manager::{PeerManager, PeerManagerConfig},
    protocols::NetworkMessage,
    transport::{TcpTransport, Transport, TransportConfig},
    NetworkConfig, NetworkEvent,
};
use async_trait::async_trait;
use setu_types::{ConsensusFrame, Event, NodeInfo, Vote};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Network client for sending messages
#[derive(Clone)]
pub struct NetworkClient {
    peer_manager: Arc<PeerManager>,
    local_node: NodeInfo,
}

impl NetworkClient {
    /// Create a new network client
    pub fn new(peer_manager: Arc<PeerManager>, local_node: NodeInfo) -> Self {
        Self {
            peer_manager,
            local_node,
        }
    }
}

#[async_trait]
impl NetworkInterface for NetworkClient {
    async fn broadcast_event(&self, event: Event) -> Result<()> {
        let message = NetworkMessage::event_broadcast(event, self.local_node.id.clone());
        self.peer_manager.broadcast(message).await
    }
    
    async fn broadcast_cf_proposal(&self, cf: ConsensusFrame, proposer_id: String) -> Result<()> {
        let message = NetworkMessage::cf_proposal(cf, proposer_id);
        self.peer_manager.send_to_validators(message).await
    }
    
    async fn broadcast_vote(&self, vote: Vote) -> Result<()> {
        let message = NetworkMessage::cf_vote(vote);
        self.peer_manager.send_to_validators(message).await
    }
    
    async fn broadcast_cf_finalized(&self, cf: ConsensusFrame) -> Result<()> {
        let message = NetworkMessage::cf_finalized(cf);
        self.peer_manager.broadcast(message).await
    }
    
    async fn send_to_peer(&self, peer_id: &str, message: NetworkMessage) -> Result<()> {
        self.peer_manager.send_to_peer(peer_id, message).await
    }
    
    async fn send_to_validators(&self, message: NetworkMessage) -> Result<()> {
        self.peer_manager.send_to_validators(message).await
    }
    
    async fn get_peer_count(&self) -> usize {
        self.peer_manager.connected_peer_count().await
    }
    
    async fn get_validator_ids(&self) -> Vec<String> {
        self.peer_manager
            .get_validators()
            .await
            .iter()
            .map(|v| v.node_info.id.clone())
            .collect()
    }
    
    async fn get_solver_ids(&self) -> Vec<String> {
        self.peer_manager
            .get_solvers()
            .await
            .iter()
            .map(|s| s.node_info.id.clone())
            .collect()
    }
}

/// Network service
pub struct NetworkService {
    config: NetworkConfig,
    local_node: NodeInfo,
    peer_manager: Arc<PeerManager>,
    transport: Arc<dyn Transport>,
    connectivity_tx: mpsc::Sender<ConnectivityRequest>,
    event_tx: mpsc::Sender<NetworkEvent>,
}

impl NetworkService {
    /// Create a new network service
    pub fn new(
        config: NetworkConfig,
        local_node: NodeInfo,
        event_tx: mpsc::Sender<NetworkEvent>,
    ) -> Self {
        // Create peer manager
        let peer_manager_config = PeerManagerConfig {
            max_peers: config.max_peers,
            max_inbound_connections: config.max_inbound_connections,
            max_outbound_connections: config.max_outbound_connections,
        };
        let peer_manager = Arc::new(PeerManager::new(peer_manager_config));
        
        // Create transport
        let transport_config = TransportConfig {
            listen_addr: config.listen_addr.clone(),
            connection_timeout_secs: config.connection_timeout.as_secs(),
            keep_alive_interval_secs: config.heartbeat_interval.as_secs(),
        };
        let transport = Arc::new(TcpTransport::new(transport_config));
        
        // Create connectivity manager channel
        let (connectivity_tx, _connectivity_rx) = mpsc::channel(config.channel_size);
        
        Self {
            config,
            local_node,
            peer_manager,
            transport,
            connectivity_tx,
            event_tx,
        }
    }
    
    /// Start the network service
    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting network service on {}", self.config.listen_addr);
        
        // Start transport
        self.transport.start().await?;
        
        // Start message handler
        self.start_message_handler();
        
        // Start connection acceptor
        self.start_connection_acceptor();
        
        tracing::info!("Network service started successfully");
        Ok(())
    }
    
    /// Get a network client
    pub fn client(&self) -> NetworkClient {
        NetworkClient::new(self.peer_manager.clone(), self.local_node.clone())
    }
    
    /// Connect to a peer
    pub async fn connect_to_peer(&self, node_info: NodeInfo, role: PeerRole) -> Result<()> {
        let mut peer_info = PeerInfo::new(node_info.clone());
        peer_info.role = role;
        
        self.peer_manager.add_peer(peer_info).await?;
        
        // Send connectivity request
        self.connectivity_tx
            .send(ConnectivityRequest::ConnectToPeer(node_info))
            .await
            .map_err(|_| crate::error::NetworkError::ChannelSendError)?;
        
        Ok(())
    }
    
    /// Disconnect from a peer
    pub async fn disconnect_peer(&self, peer_id: &str) -> Result<()> {
        self.peer_manager.remove_peer(peer_id).await?;
        
        // Send event
        let _ = self
            .event_tx
            .send(NetworkEvent::PeerDisconnected(peer_id.to_string()))
            .await;
        
        Ok(())
    }
    
    fn start_message_handler(&self) {
        let _peer_manager = self.peer_manager.clone();
        let _event_tx = self.event_tx.clone();
        let _local_node_id = self.local_node.id.clone();
        
        tokio::spawn(async move {
            tracing::info!("Message handler started");
            
            // TODO: Implement actual message receiving logic
            // This would involve:
            // 1. Listening for messages from all peers
            // 2. Deserializing messages
            // 3. Routing to appropriate handlers
            // 4. Sending network events
            
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        });
    }
    
    fn start_connection_acceptor(&self) {
        let transport = self.transport.clone();
        let _peer_manager = self.peer_manager.clone();
        let _event_tx = self.event_tx.clone();
        
        tokio::spawn(async move {
            tracing::info!("Connection acceptor started");
            
            loop {
                match transport.accept().await {
                    Ok(connection) => {
                        tracing::info!("Accepted connection from {}", connection.metadata.remote_addr);
                        
                        // TODO: Perform handshake and add peer
                        // For now, just log the connection
                    }
                    Err(e) => {
                        tracing::error!("Failed to accept connection: {:?}", e);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_node_info() -> NodeInfo {
        NodeInfo::new_validator("test-node".to_string(), "127.0.0.1".to_string(), 9000)
    }
    
    #[tokio::test]
    async fn test_network_service_creation() {
        let config = NetworkConfig::default();
        let node_info = create_test_node_info();
        let (event_tx, _event_rx) = mpsc::channel(100);
        
        let service = NetworkService::new(config, node_info, event_tx);
        let client = service.client();
        
        assert_eq!(client.get_peer_count().await, 0);
    }
}
