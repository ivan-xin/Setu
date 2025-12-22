// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Example: Basic network usage

use setu_network::{NetworkConfig, NetworkService, NetworkEvent, application::NetworkInterface};
use setu_types::NodeInfo;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Create network configuration
    let config = NetworkConfig {
        listen_addr: "0.0.0.0:9000".to_string(),
        max_peers: 100,
        heartbeat_interval: Duration::from_secs(5),
        connection_timeout: Duration::from_secs(30),
        ..Default::default()
    };
    
    // Create local node info
    let local_node = NodeInfo::new_validator(
        "validator-1".to_string(),
        "0.0.0.0".to_string(),
        9000,
    );
    
    // Create event channel
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(1000);
    
    // Create network service
    let network = NetworkService::new(config, local_node, event_tx);
    
    // Start network service
    network.start().await?;
    println!("Network service started on 0.0.0.0:9000");
    
    // Get network client
    let client = network.client();
    
    // Spawn event handler
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                NetworkEvent::PeerConnected(peer_id) => {
                    println!("✓ Peer connected: {}", peer_id);
                }
                NetworkEvent::PeerDisconnected(peer_id) => {
                    println!("✗ Peer disconnected: {}", peer_id);
                }
                NetworkEvent::EventReceived { peer_id, event } => {
                    println!("📨 Event received from {}: {:?}", peer_id, event.id);
                }
                NetworkEvent::CFProposed { peer_id, cf } => {
                    println!("📋 CF proposal from {}: {:?}", peer_id, cf.id);
                }
                NetworkEvent::VoteReceived { peer_id, vote } => {
                    println!("🗳️  Vote received from {}: {:?}", peer_id, vote.validator_id);
                }
                NetworkEvent::CFFinalized { peer_id, cf } => {
                    println!("✅ CF finalized from {}: {:?}", peer_id, cf.id);
                }
                NetworkEvent::PingReceived { peer_id } => {
                    println!("🏓 Ping from {}", peer_id);
                }
            }
        }
    });
    
    // Example: Connect to other validators
    // Uncomment to connect to other nodes
    /*
    let peer_node = NodeInfo::new_validator(
        "validator-2".to_string(),
        "127.0.0.1".to_string(),
        9001,
    );
    
    network.connect_to_peer(peer_node, setu_network::peer::PeerRole::Validator).await?;
    println!("Connected to validator-2");
    */
    
    // Keep the service running
    println!("\nNetwork service is running. Press Ctrl+C to stop.");
    println!("Connected peers: {}", client.get_peer_count().await);
    
    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    println!("\nShutting down...");
    
    Ok(())
}
