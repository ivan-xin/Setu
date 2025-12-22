// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! RPC protocol for request-response message patterns

use super::ProtocolId;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Request ID type
pub type RequestId = u64;

/// RPC request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    /// Protocol identifier
    pub protocol_id: ProtocolId,
    
    /// Unique request identifier
    pub request_id: RequestId,
    
    /// Request priority (0-255, higher is more important)
    pub priority: u8,
    
    /// Request payload
    pub payload: Bytes,
}

impl RpcRequest {
    /// Create a new RPC request
    pub fn new(protocol_id: ProtocolId, request_id: RequestId, priority: u8, payload: Bytes) -> Self {
        Self {
            protocol_id,
            request_id,
            priority,
            payload,
        }
    }
    
    /// Get the size of the request
    pub fn size(&self) -> usize {
        self.payload.len()
    }
}

/// RPC response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    /// Protocol identifier
    pub protocol_id: ProtocolId,
    
    /// Request identifier this response corresponds to
    pub request_id: RequestId,
    
    /// Response priority (0-255, higher is more important)
    pub priority: u8,
    
    /// Response payload
    pub payload: Bytes,
    
    /// Whether the request was successful
    pub is_success: bool,
}

impl RpcResponse {
    /// Create a new successful RPC response
    pub fn success(protocol_id: ProtocolId, request_id: RequestId, priority: u8, payload: Bytes) -> Self {
        Self {
            protocol_id,
            request_id,
            priority,
            payload,
            is_success: true,
        }
    }
    
    /// Create a new error RPC response
    pub fn error(protocol_id: ProtocolId, request_id: RequestId, priority: u8, payload: Bytes) -> Self {
        Self {
            protocol_id,
            request_id,
            priority,
            payload,
            is_success: false,
        }
    }
    
    /// Get the size of the response
    pub fn size(&self) -> usize {
        self.payload.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rpc_request_creation() {
        let payload = Bytes::from("test");
        let req = RpcRequest::new(ProtocolId::Consensus, 1, 100, payload.clone());
        assert_eq!(req.request_id, 1);
        assert_eq!(req.priority, 100);
        assert_eq!(req.payload, payload);
    }
    
    #[test]
    fn test_rpc_response_creation() {
        let payload = Bytes::from("response");
        let resp = RpcResponse::success(ProtocolId::Consensus, 1, 100, payload.clone());
        assert_eq!(resp.request_id, 1);
        assert!(resp.is_success);
        
        let error_resp = RpcResponse::error(ProtocolId::Consensus, 1, 100, payload);
        assert!(!error_resp.is_success);
    }
}
