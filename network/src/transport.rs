// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network transport layer

use crate::error::{NetworkError, Result};
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

/// Transport configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Listen address
    pub listen_addr: String,
    
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    
    /// Keep-alive interval in seconds
    pub keep_alive_interval_secs: u64,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9000".to_string(),
            connection_timeout_secs: 30,
            keep_alive_interval_secs: 60,
        }
    }
}

/// Connection metadata
#[derive(Debug, Clone)]
pub struct ConnectionMetadata {
    /// Remote peer address
    pub remote_addr: SocketAddr,
    
    /// Whether this is an inbound connection
    pub is_inbound: bool,
}

/// Connection handle
pub struct Connection {
    /// Connection metadata
    pub metadata: ConnectionMetadata,
    
    /// Underlying TCP stream
    stream: TcpStream,
}

impl Connection {
    /// Create a new connection
    pub fn new(stream: TcpStream, is_inbound: bool) -> Result<Self> {
        let remote_addr = stream
            .peer_addr()
            .map_err(|e| NetworkError::IoError(e))?;
        
        Ok(Self {
            metadata: ConnectionMetadata {
                remote_addr,
                is_inbound,
            },
            stream,
        })
    }
    
    /// Get the underlying stream
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }
    
    /// Get mutable stream
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }
}

/// Transport trait for network communication
#[async_trait]
pub trait Transport: Send + Sync {
    /// Start listening for connections
    async fn start(&self) -> Result<()>;
    
    /// Connect to a remote peer
    async fn connect(&self, addr: &str) -> Result<Connection>;
    
    /// Accept an incoming connection
    async fn accept(&self) -> Result<Connection>;
    
    /// Close the transport
    async fn close(&self) -> Result<()>;
}

/// TCP-based transport implementation
pub struct TcpTransport {
    config: TransportConfig,
    listener: Arc<tokio::sync::Mutex<Option<TcpListener>>>,
}

impl TcpTransport {
    /// Create a new TCP transport
    pub fn new(config: TransportConfig) -> Self {
        Self {
            config,
            listener: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Transport for TcpTransport {
    async fn start(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.listen_addr)
            .await
            .map_err(|e| NetworkError::TransportError(format!("Failed to bind: {}", e)))?;
        
        tracing::info!("Transport listening on {}", self.config.listen_addr);
        
        let mut guard = self.listener.lock().await;
        *guard = Some(listener);
        
        Ok(())
    }
    
    async fn connect(&self, addr: &str) -> Result<Connection> {
        let timeout = std::time::Duration::from_secs(self.config.connection_timeout_secs);
        
        let stream = tokio::time::timeout(timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| NetworkError::TimeoutError(format!("Connection to {} timed out", addr)))?
            .map_err(|e| NetworkError::ConnectionError(format!("Failed to connect to {}: {}", addr, e)))?;
        
        Connection::new(stream, false)
    }
    
    async fn accept(&self) -> Result<Connection> {
        let guard = self.listener.lock().await;
        let listener = guard
            .as_ref()
            .ok_or_else(|| NetworkError::TransportError("Transport not started".to_string()))?;
        
        let (stream, _addr) = listener
            .accept()
            .await
            .map_err(|e| NetworkError::IoError(e))?;
        
        Connection::new(stream, true)
    }
    
    async fn close(&self) -> Result<()> {
        let mut guard = self.listener.lock().await;
        *guard = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_tcp_transport_start() {
        let config = TransportConfig {
            listen_addr: "127.0.0.1:0".to_string(),
            ..Default::default()
        };
        
        let transport = TcpTransport::new(config);
        let result = transport.start().await;
        assert!(result.is_ok());
    }
}
