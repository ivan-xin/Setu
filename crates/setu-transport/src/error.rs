//! Transport layer errors

use thiserror::Error;

/// Transport layer error types
#[derive(Error, Debug)]
pub enum TransportError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    HttpRequestFailed(String),

    /// Connection refused
    #[error("Connection refused: {0}")]
    ConnectionRefused(String),

    /// Timeout
    #[error("Request timeout: {0}")]
    Timeout(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    /// Invalid response
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Server error
    #[error("Server error ({code}): {message}")]
    ServerError { code: u16, message: String },

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type alias for transport operations
pub type TransportResult<T> = Result<T, TransportError>;

impl From<serde_json::Error> for TransportError {
    fn from(e: serde_json::Error) -> Self {
        TransportError::SerializationError(e.to_string())
    }
}

#[cfg(feature = "http")]
impl From<reqwest::Error> for TransportError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_connect() {
            TransportError::ConnectionRefused(e.to_string())
        } else if e.is_timeout() {
            TransportError::Timeout(e.to_string())
        } else {
            TransportError::HttpRequestFailed(e.to_string())
        }
    }
}
