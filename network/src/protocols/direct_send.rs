// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Direct send protocol for fire-and-forget message delivery

use super::ProtocolId;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Direct send message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectSendMsg {
    /// Protocol identifier
    pub protocol_id: ProtocolId,
    
    /// Message priority (0-255, higher is more important)
    pub priority: u8,
    
    /// Message payload
    pub payload: Bytes,
}

impl DirectSendMsg {
    /// Create a new direct send message
    pub fn new(protocol_id: ProtocolId, priority: u8, payload: Bytes) -> Self {
        Self {
            protocol_id,
            priority,
            payload,
        }
    }
    
    /// Get the size of the message
    pub fn size(&self) -> usize {
        self.payload.len()
    }
}
