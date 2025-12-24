// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Error types for Anemo network implementation

use thiserror::Error;

/// Result type alias for Anemo network operations
pub type Result<T> = std::result::Result<T, AnemoError>;

/// Error types for Anemo network operations
#[derive(Error, Debug)]
pub enum AnemoError {
    #[error("Anemo network error: {0}")]
    Network(String),

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("RPC timeout: {0}")]
    RpcTimeout(String),

    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("Send error: {0}")]
    SendError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Network shutdown")]
    NetworkShutdown,

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<anemo::Error> for AnemoError {
    fn from(err: anemo::Error) -> Self {
        AnemoError::Network(err.to_string())
    }
}

impl From<bincode::Error> for AnemoError {
    fn from(err: bincode::Error) -> Self {
        AnemoError::Serialization(err.to_string())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for AnemoError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        AnemoError::ChannelError(err.to_string())
    }
}
