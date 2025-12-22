// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Handshake protocol for connection establishment

use crate::constants::NETWORK_PROTOCOL_VERSION;
use serde::{Deserialize, Serialize};
use setu_types::NodeInfo;

/// Handshake message for connection establishment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeMessage {
    /// Protocol version
    pub version: u32,
    
    /// Node information
    pub node_info: NodeInfo,
    
    /// Supported protocols
    pub supported_protocols: Vec<u8>,
    
    /// Network chain ID (for compatibility checking)
    pub chain_id: String,
}

impl HandshakeMessage {
    /// Create a new handshake message
    pub fn new(node_info: NodeInfo, chain_id: String) -> Self {
        Self {
            version: NETWORK_PROTOCOL_VERSION,
            node_info,
            supported_protocols: vec![0, 1, 2, 3, 4], // All protocols
            chain_id,
        }
    }
    
    /// Check if the handshake is compatible
    pub fn is_compatible(&self, other: &HandshakeMessage) -> bool {
        // Check protocol version
        if self.version != other.version {
            return false;
        }
        
        // Check chain ID
        if self.chain_id != other.chain_id {
            return false;
        }
        
        // Check if there are any common protocols
        for protocol in &self.supported_protocols {
            if other.supported_protocols.contains(protocol) {
                return true;
            }
        }
        
        false
    }
}

/// Handshake handler
pub struct Handshake {
    local_message: HandshakeMessage,
}

impl Handshake {
    /// Create a new handshake handler
    pub fn new(node_info: NodeInfo, chain_id: String) -> Self {
        Self {
            local_message: HandshakeMessage::new(node_info, chain_id),
        }
    }
    
    /// Get the local handshake message
    pub fn local_message(&self) -> &HandshakeMessage {
        &self.local_message
    }
    
    /// Perform handshake verification
    pub fn verify(&self, remote_message: &HandshakeMessage) -> Result<(), String> {
        if !self.local_message.is_compatible(remote_message) {
            return Err(format!(
                "Incompatible handshake: version={}, chain_id={}",
                remote_message.version, remote_message.chain_id
            ));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_node_info(id: &str, port: u16) -> NodeInfo {
        NodeInfo::new_validator(id.to_string(), "127.0.0.1".to_string(), port)
    }
    
    #[test]
    fn test_handshake_compatibility() {
        let node_info1 = create_test_node_info("node1", 9000);
        let node_info2 = create_test_node_info("node2", 9001);
        
        let msg1 = HandshakeMessage::new(node_info1, "test-chain".to_string());
        let msg2 = HandshakeMessage::new(node_info2, "test-chain".to_string());
        
        assert!(msg1.is_compatible(&msg2));
    }
    
    #[test]
    fn test_handshake_incompatible_chain() {
        let node_info1 = create_test_node_info("node1", 9000);
        let node_info2 = create_test_node_info("node2", 9001);
        
        let msg1 = HandshakeMessage::new(node_info1, "chain1".to_string());
        let msg2 = HandshakeMessage::new(node_info2, "chain2".to_string());
        
        assert!(!msg1.is_compatible(&msg2));
    }
}
