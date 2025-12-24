// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Transport layer metrics

use prometheus::{Histogram, HistogramOpts, IntCounter, IntGauge, Registry};

/// Metrics for the transport layer
pub struct TransportMetrics {
    /// Active QUIC connections
    pub active_connections: IntGauge,
    /// Total connections established
    pub connections_established: IntCounter,
    /// Total connections closed
    pub connections_closed: IntCounter,
    /// Connection failures
    pub connection_failures: IntCounter,
    /// Bytes sent
    pub bytes_sent: IntCounter,
    /// Bytes received
    pub bytes_received: IntCounter,
    /// RPC requests sent
    pub rpc_requests_sent: IntCounter,
    /// RPC requests received
    pub rpc_requests_received: IntCounter,
    /// RPC latency (seconds)
    pub rpc_latency: Histogram,
    /// RPC errors
    pub rpc_errors: IntCounter,
}

impl TransportMetrics {
    /// Create metrics registered with the given registry
    pub fn new(registry: &Registry) -> Self {
        Self {
            active_connections: register_int_gauge(
                registry,
                "transport_active_connections",
                "Number of active QUIC connections",
            ),
            connections_established: register_int_counter(
                registry,
                "transport_connections_established_total",
                "Total connections established",
            ),
            connections_closed: register_int_counter(
                registry,
                "transport_connections_closed_total",
                "Total connections closed",
            ),
            connection_failures: register_int_counter(
                registry,
                "transport_connection_failures_total",
                "Connection failures",
            ),
            bytes_sent: register_int_counter(
                registry,
                "transport_bytes_sent_total",
                "Total bytes sent",
            ),
            bytes_received: register_int_counter(
                registry,
                "transport_bytes_received_total",
                "Total bytes received",
            ),
            rpc_requests_sent: register_int_counter(
                registry,
                "transport_rpc_sent_total",
                "RPC requests sent",
            ),
            rpc_requests_received: register_int_counter(
                registry,
                "transport_rpc_received_total",
                "RPC requests received",
            ),
            rpc_latency: register_histogram(
                registry,
                "transport_rpc_latency_seconds",
                "RPC request latency",
            ),
            rpc_errors: register_int_counter(
                registry,
                "transport_rpc_errors_total",
                "RPC request errors",
            ),
        }
    }

    /// Create disabled (no-op) metrics
    pub fn disabled() -> Self {
        Self {
            active_connections: IntGauge::new("disabled", "disabled").unwrap(),
            connections_established: IntCounter::new("disabled", "disabled").unwrap(),
            connections_closed: IntCounter::new("disabled", "disabled").unwrap(),
            connection_failures: IntCounter::new("disabled", "disabled").unwrap(),
            bytes_sent: IntCounter::new("disabled", "disabled").unwrap(),
            bytes_received: IntCounter::new("disabled", "disabled").unwrap(),
            rpc_requests_sent: IntCounter::new("disabled", "disabled").unwrap(),
            rpc_requests_received: IntCounter::new("disabled", "disabled").unwrap(),
            rpc_latency: Histogram::with_opts(HistogramOpts::new("disabled", "disabled")).unwrap(),
            rpc_errors: IntCounter::new("disabled", "disabled").unwrap(),
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

fn register_histogram(registry: &Registry, name: &str, help: &str) -> Histogram {
    let histogram = Histogram::with_opts(HistogramOpts::new(name, help)).unwrap();
    registry.register(Box::new(histogram.clone())).unwrap();
    histogram
}
