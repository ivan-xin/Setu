// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Metrics collection for Setu network
//!
//! This module provides Prometheus metrics for monitoring the network
//! layer. Metrics are organized by subsystem:
//!
//! - Discovery metrics: peer discovery and connection tracking
//! - State sync metrics: synchronization progress and latency
//! - Transport metrics: connection and RPC performance
//!
//! ## Usage
//!
//! ```ignore
//! use setu_network_anemo::metrics::NetworkMetrics;
//!
//! let registry = prometheus::Registry::new();
//! let metrics = NetworkMetrics::new(&registry);
//!
//! // Use with network service
//! let service = NetworkServiceBuilder::new()
//!     .with_metrics(metrics)
//!     .build();
//! ```

mod transport_metrics;

pub use crate::discovery::metrics::DiscoveryMetrics;
pub use crate::state_sync::metrics::StateSyncMetrics;
pub use transport_metrics::TransportMetrics;

use prometheus::Registry;

/// Combined metrics for all network subsystems
pub struct NetworkMetrics {
    pub discovery: DiscoveryMetrics,
    pub state_sync: StateSyncMetrics,
    pub transport: TransportMetrics,
}

impl NetworkMetrics {
    /// Create metrics registered with the given registry
    pub fn new(registry: &Registry) -> Self {
        Self {
            discovery: DiscoveryMetrics::new(registry),
            state_sync: StateSyncMetrics::new(registry),
            transport: TransportMetrics::new(registry),
        }
    }

    /// Create disabled (no-op) metrics
    pub fn disabled() -> Self {
        Self {
            discovery: DiscoveryMetrics::disabled(),
            state_sync: StateSyncMetrics::disabled(),
            transport: TransportMetrics::disabled(),
        }
    }
}
