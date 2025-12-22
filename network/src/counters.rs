// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network metrics and counters

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Network metrics collector
#[derive(Debug, Clone)]
pub struct NetworkMetrics {
    inner: Arc<NetworkMetricsInner>,
}

#[derive(Debug)]
struct NetworkMetricsInner {
    // Connection metrics
    total_connections: AtomicU64,
    active_connections: AtomicU64,
    inbound_connections: AtomicU64,
    outbound_connections: AtomicU64,
    failed_connections: AtomicU64,
    
    // Message metrics
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    
    // Error metrics
    send_errors: AtomicU64,
    receive_errors: AtomicU64,
    timeout_errors: AtomicU64,
    
    // Protocol metrics
    direct_send_messages: AtomicU64,
    rpc_requests: AtomicU64,
    rpc_responses: AtomicU64,
}

impl NetworkMetrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            inner: Arc::new(NetworkMetricsInner {
                total_connections: AtomicU64::new(0),
                active_connections: AtomicU64::new(0),
                inbound_connections: AtomicU64::new(0),
                outbound_connections: AtomicU64::new(0),
                failed_connections: AtomicU64::new(0),
                messages_sent: AtomicU64::new(0),
                messages_received: AtomicU64::new(0),
                bytes_sent: AtomicU64::new(0),
                bytes_received: AtomicU64::new(0),
                send_errors: AtomicU64::new(0),
                receive_errors: AtomicU64::new(0),
                timeout_errors: AtomicU64::new(0),
                direct_send_messages: AtomicU64::new(0),
                rpc_requests: AtomicU64::new(0),
                rpc_responses: AtomicU64::new(0),
            }),
        }
    }
    
    // Connection metrics
    pub fn inc_total_connections(&self) {
        self.inner.total_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn inc_active_connections(&self) {
        self.inner.active_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn dec_active_connections(&self) {
        self.inner.active_connections.fetch_sub(1, Ordering::Relaxed);
    }
    
    pub fn inc_inbound_connections(&self) {
        self.inner.inbound_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn dec_inbound_connections(&self) {
        self.inner.inbound_connections.fetch_sub(1, Ordering::Relaxed);
    }
    
    pub fn inc_outbound_connections(&self) {
        self.inner.outbound_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn dec_outbound_connections(&self) {
        self.inner.outbound_connections.fetch_sub(1, Ordering::Relaxed);
    }
    
    pub fn inc_failed_connections(&self) {
        self.inner.failed_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    // Message metrics
    pub fn inc_messages_sent(&self) {
        self.inner.messages_sent.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn inc_messages_received(&self) {
        self.inner.messages_received.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn add_bytes_sent(&self, bytes: u64) {
        self.inner.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }
    
    pub fn add_bytes_received(&self, bytes: u64) {
        self.inner.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }
    
    // Error metrics
    pub fn inc_send_errors(&self) {
        self.inner.send_errors.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn inc_receive_errors(&self) {
        self.inner.receive_errors.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn inc_timeout_errors(&self) {
        self.inner.timeout_errors.fetch_add(1, Ordering::Relaxed);
    }
    
    // Protocol metrics
    pub fn inc_direct_send_messages(&self) {
        self.inner.direct_send_messages.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn inc_rpc_requests(&self) {
        self.inner.rpc_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn inc_rpc_responses(&self) {
        self.inner.rpc_responses.fetch_add(1, Ordering::Relaxed);
    }
    
    // Getters
    pub fn get_active_connections(&self) -> u64 {
        self.inner.active_connections.load(Ordering::Relaxed)
    }
    
    pub fn get_messages_sent(&self) -> u64 {
        self.inner.messages_sent.load(Ordering::Relaxed)
    }
    
    pub fn get_messages_received(&self) -> u64 {
        self.inner.messages_received.load(Ordering::Relaxed)
    }
    
    pub fn get_bytes_sent(&self) -> u64 {
        self.inner.bytes_sent.load(Ordering::Relaxed)
    }
    
    pub fn get_bytes_received(&self) -> u64 {
        self.inner.bytes_received.load(Ordering::Relaxed)
    }
}

impl Default for NetworkMetrics {
    fn default() -> Self {
        Self::new()
    }
}
