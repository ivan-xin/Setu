// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Metrics for discovery service

use prometheus::{IntCounter, IntGauge, Registry};

/// Metrics for the discovery service
pub struct DiscoveryMetrics {
    /// Number of connected peers
    pub connected_peers: IntGauge,
    /// Number of connected validators
    pub connected_validators: IntGauge,
    /// Number of connected solvers
    pub connected_solvers: IntGauge,
    /// Total known peers
    pub known_peers: IntGauge,
    /// Discovery ticks processed
    pub ticks_processed: IntCounter,
    /// Peer exchange requests sent
    pub peer_exchange_requests_sent: IntCounter,
    /// Peer exchange requests received
    pub peer_exchange_requests_received: IntCounter,
    /// New peers discovered
    pub new_peers_discovered: IntCounter,
}

impl DiscoveryMetrics {
    /// Create metrics registered with the given registry
    pub fn new(registry: &Registry) -> Self {
        Self {
            connected_peers: register_int_gauge(
                registry,
                "discovery_connected_peers",
                "Number of currently connected peers",
            ),
            connected_validators: register_int_gauge(
                registry,
                "discovery_connected_validators",
                "Number of connected validator nodes",
            ),
            connected_solvers: register_int_gauge(
                registry,
                "discovery_connected_solvers",
                "Number of connected solver nodes",
            ),
            known_peers: register_int_gauge(
                registry,
                "discovery_known_peers",
                "Total number of known peers (connected + disconnected)",
            ),
            ticks_processed: register_int_counter(
                registry,
                "discovery_ticks_total",
                "Total discovery ticks processed",
            ),
            peer_exchange_requests_sent: register_int_counter(
                registry,
                "discovery_peer_exchange_sent_total",
                "Peer exchange requests sent",
            ),
            peer_exchange_requests_received: register_int_counter(
                registry,
                "discovery_peer_exchange_received_total",
                "Peer exchange requests received",
            ),
            new_peers_discovered: register_int_counter(
                registry,
                "discovery_new_peers_total",
                "New peers discovered",
            ),
        }
    }

    /// Create disabled (no-op) metrics
    pub fn disabled() -> Self {
        Self {
            connected_peers: IntGauge::new("disabled", "disabled").unwrap(),
            connected_validators: IntGauge::new("disabled", "disabled").unwrap(),
            connected_solvers: IntGauge::new("disabled", "disabled").unwrap(),
            known_peers: IntGauge::new("disabled", "disabled").unwrap(),
            ticks_processed: IntCounter::new("disabled", "disabled").unwrap(),
            peer_exchange_requests_sent: IntCounter::new("disabled", "disabled").unwrap(),
            peer_exchange_requests_received: IntCounter::new("disabled", "disabled").unwrap(),
            new_peers_discovered: IntCounter::new("disabled", "disabled").unwrap(),
        }
    }
}

fn register_int_gauge(registry: &Registry, name: &str, help: &str) -> IntGauge {
    let gauge = IntGauge::new(name, help).unwrap();
    registry.register(Box::new(gauge.clone())).unwrap();
    gauge
}

fn register_int_counter(registry: &Registry, name: &str, help: &str) -> IntCounter {
    let counter = IntCounter::new(name, help).unwrap();
    registry.register(Box::new(counter.clone())).unwrap();
    counter
}
