// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Message codec for framing and serialization

use crate::error::{NetworkError, Result};
use crate::protocols::NetworkMessage;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

/// Maximum frame size (8MB)
const MAX_FRAME_SIZE: usize = 8 * 1024 * 1024;

/// Message frame wrapper
#[derive(Debug, Clone)]
pub struct MessageFrame {
    /// Message payload
    pub payload: Bytes,
}

impl MessageFrame {
    /// Create a new message frame
    pub fn new(payload: Bytes) -> Self {
        Self { payload }
    }
    
    /// Get the size of the frame
    pub fn size(&self) -> usize {
        self.payload.len()
    }
}

/// Codec for encoding/decoding network messages
pub struct MessageCodec {
    max_frame_size: usize,
}

impl MessageCodec {
    /// Create a new message codec
    pub fn new(max_frame_size: usize) -> Self {
        Self {
            max_frame_size: max_frame_size.min(MAX_FRAME_SIZE),
        }
    }
}

impl Default for MessageCodec {
    fn default() -> Self {
        Self::new(MAX_FRAME_SIZE)
    }
}

impl Decoder for MessageCodec {
    type Item = NetworkMessage;
    type Error = NetworkError;
    
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        // Need at least 4 bytes for length prefix
        if src.len() < 4 {
            return Ok(None);
        }
        
        // Read length prefix
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_be_bytes(length_bytes) as usize;
        
        // Check if frame is too large
        if length > self.max_frame_size {
            return Err(NetworkError::InvalidMessage(format!(
                "Frame size {} exceeds maximum {}",
                length, self.max_frame_size
            )));
        }
        
        // Check if we have the full message
        if src.len() < 4 + length {
            // Reserve space for the full message
            src.reserve(4 + length - src.len());
            return Ok(None);
        }
        
        // Skip length prefix
        src.advance(4);
        
        // Extract message bytes
        let message_bytes = src.split_to(length);
        
        // Deserialize message
        let message = bincode::deserialize(&message_bytes)
            .map_err(|e| NetworkError::DeserializationError(e.to_string()))?;
        
        Ok(Some(message))
    }
}

impl Encoder<NetworkMessage> for MessageCodec {
    type Error = NetworkError;
    
    fn encode(&mut self, item: NetworkMessage, dst: &mut BytesMut) -> Result<()> {
        // Serialize message
        let message_bytes = bincode::serialize(&item)
            .map_err(|e| NetworkError::SerializationError(e.to_string()))?;
        
        // Check size
        if message_bytes.len() > self.max_frame_size {
            return Err(NetworkError::InvalidMessage(format!(
                "Message size {} exceeds maximum {}",
                message_bytes.len(),
                self.max_frame_size
            )));
        }
        
        // Write length prefix
        let length = message_bytes.len() as u32;
        dst.reserve(4 + message_bytes.len());
        dst.put_u32(length);
        
        // Write message
        dst.put_slice(&message_bytes);
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_codec_encode_decode() {
        let mut codec = MessageCodec::default();
        let message = NetworkMessage::ping(123, 456);
        
        // Encode
        let mut buf = BytesMut::new();
        codec.encode(message.clone(), &mut buf).unwrap();
        
        // Decode
        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.protocol_id(), message.protocol_id());
    }
}
