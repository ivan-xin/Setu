// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! # Setu Network Module
//!
//! This module provides the networking layer for Setu, enabling communication
//! between validators and solvers in the distributed system.
//!
//! ## Architecture
//!
//! The network is designed with the following components:
//! - **PeerManager**: Manages peer connections and lifecycle
//! - **Transport**: Handles low-level networking (QUIC-based)
//! - **Protocols**: DirectSend and RPC for different message patterns
//! - **ConnectivityManager**: Maintains network topology and health

pub mod application;
pub mod connectivity_manager;
pub mod constants;
pub mod counters;
pub mod error;
pub mod peer;
pub mod peer_manager;
pub mod protocols;
pub mod transport;

pub use application::{NetworkClient, NetworkInterface, NetworkService};
pub use connectivity_manager::{ConnectivityManager, ConnectivityRequest};
pub use error::{NetworkError, Result};
pub use peer::{Peer, PeerInfo, PeerRole, PeerState};
pub use peer_manager::{PeerManager, PeerManagerConfig};
pub use protocols::{DirectSendMsg, ProtocolId, RpcRequest, RpcResponse};
pub use transport::{Connection, Transport, TransportConfig};

use std::time::Duration;

/// Network configuration
#[derive(Clone, Debug)]
pub struct NetworkConfig {
    /// Address to listen on for incoming connections
    pub listen_addr: String,
    
    /// Maximum number of peers to connect to
    pub max_peers: usize,
    
    /// Maximum number of inbound connections
    pub max_inbound_connections: usize,
    
    /// Maximum number of outbound connections
    pub max_outbound_connections: usize,
    
    /// Heartbeat interval for liveness checks
    pub heartbeat_interval: Duration,
    
    /// Connection timeout
    pub connection_timeout: Duration,
    
    /// Request timeout for RPC calls
    pub request_timeout: Duration,
    
    /// Maximum frame size for messages
    pub max_frame_size: usize,
    
    /// Maximum message size
    pub max_message_size: usize,
    
    /// Channel buffer size
    pub channel_size: usize,
    
    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9000".to_string(),
            max_peers: 100,
            max_inbound_connections: 50,
            max_outbound_connections: 50,
            heartbeat_interval: Duration::from_secs(5),
            connection_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
            max_frame_size: 8 * 1024 * 1024, // 8MB
            max_message_size: 4 * 1024 * 1024, // 4MB
            channel_size: 1024,
            enable_metrics: true,
        }
    }
}

/// Network events that can be received by applications
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// A new peer has connected
    PeerConnected(String),
    
    /// A peer has disconnected
    PeerDisconnected(String),
    
    /// An event was received from a peer
    EventReceived {
        peer_id: String,
        event: setu_types::Event,
    },
    
    /// A consensus frame proposal was received
    CFProposed {
        peer_id: String,
        cf: setu_types::ConsensusFrame,
    },
    
    /// A vote was received
    VoteReceived {
        peer_id: String,
        vote: setu_types::Vote,
    },
    
    /// A consensus frame was finalized
    CFFinalized {
        peer_id: String,
        cf: setu_types::ConsensusFrame,
    },
    
    /// A ping was received
    PingReceived {
        peer_id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NetworkConfig::default();
        assert_eq!(config.listen_addr, "0.0.0.0:9000");
        assert_eq!(config.max_peers, 100);
        assert!(config.enable_metrics);
    }
}
