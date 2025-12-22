// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Connectivity manager for maintaining network topology

use crate::{
    error::Result,
    peer::{PeerInfo, PeerRole},
    peer_manager::PeerManager,
    transport::Transport,
};
use setu_types::NodeInfo;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Connectivity request types
#[derive(Debug)]
pub enum ConnectivityRequest {
    /// Connect to a peer
    ConnectToPeer(NodeInfo),
    
    /// Disconnect from a peer
    DisconnectFromPeer(String),
    
    /// Update validator set
    UpdateValidatorSet(Vec<NodeInfo>),
    
    /// Update solver set
    UpdateSolverSet(Vec<NodeInfo>),
}

/// Connectivity manager configuration
#[derive(Debug, Clone)]
pub struct ConnectivityConfig {
    /// Interval for checking peer health
    pub health_check_interval: Duration,
    
    /// Interval for reconnecting to disconnected peers
    pub reconnect_interval: Duration,
    
    /// Maximum reconnect attempts
    pub max_reconnect_attempts: usize,
}

impl Default for ConnectivityConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(10),
            reconnect_interval: Duration::from_secs(30),
            max_reconnect_attempts: 5,
        }
    }
}

/// Connectivity manager for maintaining network topology
pub struct ConnectivityManager {
    peer_manager: Arc<PeerManager>,
    transport: Arc<dyn Transport>,
    config: ConnectivityConfig,
    request_rx: mpsc::Receiver<ConnectivityRequest>,
}

impl ConnectivityManager {
    /// Create a new connectivity manager
    pub fn new(
        peer_manager: Arc<PeerManager>,
        transport: Arc<dyn Transport>,
        config: ConnectivityConfig,
        request_rx: mpsc::Receiver<ConnectivityRequest>,
    ) -> Self {
        Self {
            peer_manager,
            transport,
            config,
            request_rx,
        }
    }
    
    /// Start the connectivity manager
    pub async fn run(mut self) {
        tracing::info!("Connectivity manager started");
        
        let mut health_check_interval = tokio::time::interval(self.config.health_check_interval);
        let mut reconnect_interval = tokio::time::interval(self.config.reconnect_interval);
        
        loop {
            tokio::select! {
                Some(request) = self.request_rx.recv() => {
                    self.handle_request(request).await;
                }
                _ = health_check_interval.tick() => {
                    self.perform_health_check().await;
                }
                _ = reconnect_interval.tick() => {
                    self.attempt_reconnections().await;
                }
            }
        }
    }
    
    async fn handle_request(&mut self, request: ConnectivityRequest) {
        match request {
            ConnectivityRequest::ConnectToPeer(node_info) => {
                let _ = self.connect_to_peer(node_info).await;
            }
            ConnectivityRequest::DisconnectFromPeer(peer_id) => {
                let _ = self.peer_manager.remove_peer(&peer_id).await;
            }
            ConnectivityRequest::UpdateValidatorSet(validators) => {
                self.update_validator_set(validators).await;
            }
            ConnectivityRequest::UpdateSolverSet(solvers) => {
                self.update_solver_set(solvers).await;
            }
        }
    }
    
    async fn connect_to_peer(&self, node_info: NodeInfo) -> Result<()> {
        tracing::info!("Connecting to peer: {}", node_info.id);
        
        // Create peer info
        let peer_info = PeerInfo::new(node_info.clone());
        
        // Add peer to manager
        let peer = self.peer_manager.add_peer(peer_info).await?;
        
        // Attempt connection
        match self.transport.connect(&node_info.address).await {
            Ok(_connection) => {
                peer.mark_connected().await;
                tracing::info!("Successfully connected to peer: {}", node_info.id);
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to connect to peer {}: {:?}", node_info.id, e);
                let _ = self.peer_manager.remove_peer(&node_info.id).await;
                Err(e)
            }
        }
    }
    
    async fn update_validator_set(&self, validators: Vec<NodeInfo>) {
        tracing::info!("Updating validator set with {} validators", validators.len());
        
        for node_info in validators {
            let mut peer_info = PeerInfo::new(node_info);
            peer_info.role = PeerRole::Validator;
            
            if let Err(e) = self.peer_manager.add_peer(peer_info).await {
                tracing::warn!("Failed to add validator: {:?}", e);
            }
        }
    }
    
    async fn update_solver_set(&self, solvers: Vec<NodeInfo>) {
        tracing::info!("Updating solver set with {} solvers", solvers.len());
        
        for node_info in solvers {
            let mut peer_info = PeerInfo::new(node_info);
            peer_info.role = PeerRole::Solver;
            
            if let Err(e) = self.peer_manager.add_peer(peer_info).await {
                tracing::warn!("Failed to add solver: {:?}", e);
            }
        }
    }
    
    async fn perform_health_check(&self) {
        // TODO: Implement health check logic
        // - Send ping to all connected peers
        // - Check for unresponsive peers
        // - Disconnect unhealthy peers
        tracing::debug!("Performing health check");
    }
    
    async fn attempt_reconnections(&self) {
        // TODO: Implement reconnection logic
        // - Find disconnected peers
        // - Attempt to reconnect
        // - Track reconnection attempts
        tracing::debug!("Attempting reconnections");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer_manager::PeerManagerConfig;
    use crate::transport::{TcpTransport, TransportConfig};
    
    #[tokio::test]
    async fn test_connectivity_manager_creation() {
        let peer_manager = Arc::new(PeerManager::new(PeerManagerConfig::default()));
        let transport = Arc::new(TcpTransport::new(TransportConfig::default()));
        let (_tx, rx) = mpsc::channel(100);
        
        let _manager = ConnectivityManager::new(
            peer_manager,
            transport,
            ConnectivityConfig::default(),
            rx,
        );
    }
}
