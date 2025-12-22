// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network error types

use thiserror::Error;

/// Network result type
pub type Result<T> = std::result::Result<T, NetworkError>;

/// Network error types
#[derive(Debug, Error)]
pub enum NetworkError {
    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    /// Connection error
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    /// Peer not found
    #[error("Peer not found: {0}")]
    PeerNotFound(String),
    
    /// Peer already exists
    #[error("Peer already exists: {0}")]
    PeerAlreadyExists(String),
    
    /// Maximum peers reached
    #[error("Maximum peers reached: {0}")]
    MaxPeersReached(usize),
    
    /// Protocol error
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// Deserialization error
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
    
    /// Timeout error
    #[error("Timeout error: {0}")]
    TimeoutError(String),
    
    /// Invalid message
    #[error("Invalid message: {0}")]
    InvalidMessage(String),
    
    /// Channel send error
    #[error("Channel send error")]
    ChannelSendError,
    
    /// Channel receive error
    #[error("Channel receive error")]
    ChannelRecvError,
    
    /// Not connected
    #[error("Not connected to peer: {0}")]
    NotConnected(String),
    
    /// Already connected
    #[error("Already connected to peer: {0}")]
    AlreadyConnected(String),
    
    /// Transport error
    #[error("Transport error: {0}")]
    TransportError(String),
    
    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    /// Invalid peer role
    #[error("Invalid peer role: expected {expected}, got {actual}")]
    InvalidPeerRole { expected: String, actual: String },
    
    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    /// Unexpected error
    #[error("Unexpected error: {0}")]
    Unexpected(String),
}

impl From<bincode::Error> for NetworkError {
    fn from(err: bincode::Error) -> Self {
        NetworkError::SerializationError(err.to_string())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for NetworkError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        NetworkError::ChannelSendError
    }
}
