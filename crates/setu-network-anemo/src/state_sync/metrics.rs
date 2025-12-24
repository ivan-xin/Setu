// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Metrics for state synchronization

use prometheus::{Histogram, HistogramOpts, IntCounter, IntGauge, Registry};

/// Metrics for the state sync service
pub struct StateSyncMetrics {
    /// Current highest event sequence
    pub highest_event_seq: IntGauge,
    /// Current highest finalized CF
    pub highest_finalized_cf: IntGauge,
    /// Current highest verified CF
    pub highest_verified_cf: IntGauge,
    /// Number of peers we're syncing with
    pub syncing_peers: IntGauge,
    /// Total events synced
    pub events_synced: IntCounter,
    /// Total CFs synced
    pub cfs_synced: IntCounter,
    /// Sync requests sent
    pub sync_requests_sent: IntCounter,
    /// Sync requests received
    pub sync_requests_received: IntCounter,
    /// Sync latency histogram (seconds)
    pub sync_latency: Histogram,
    /// Events behind best peer
    pub events_behind: IntGauge,
    /// CFs behind best peer
    pub cfs_behind: IntGauge,
}

impl StateSyncMetrics {
    /// Create metrics registered with the given registry
    pub fn new(registry: &Registry) -> Self {
        Self {
            highest_event_seq: register_int_gauge(
                registry,
                "state_sync_highest_event_seq",
                "Highest event sequence number",
            ),
            highest_finalized_cf: register_int_gauge(
                registry,
                "state_sync_highest_finalized_cf",
                "Highest finalized consensus frame",
            ),
            highest_verified_cf: register_int_gauge(
                registry,
                "state_sync_highest_verified_cf",
                "Highest verified consensus frame",
            ),
            syncing_peers: register_int_gauge(
                registry,
                "state_sync_peers",
                "Number of peers currently syncing with",
            ),
            events_synced: register_int_counter(
                registry,
                "state_sync_events_total",
                "Total events synced from peers",
            ),
            cfs_synced: register_int_counter(
                registry,
                "state_sync_cfs_total",
                "Total consensus frames synced",
            ),
            sync_requests_sent: register_int_counter(
                registry,
                "state_sync_requests_sent_total",
                "Sync requests sent to peers",
            ),
            sync_requests_received: register_int_counter(
                registry,
                "state_sync_requests_received_total",
                "Sync requests received from peers",
            ),
            sync_latency: register_histogram(
                registry,
                "state_sync_latency_seconds",
                "Time to complete sync operations",
            ),
            events_behind: register_int_gauge(
                registry,
                "state_sync_events_behind",
                "Events behind best peer",
            ),
            cfs_behind: register_int_gauge(
                registry,
                "state_sync_cfs_behind",
                "CFs behind best peer",
            ),
        }
    }

    /// Create disabled (no-op) metrics
    pub fn disabled() -> Self {
        Self {
            highest_event_seq: IntGauge::new("disabled", "disabled").unwrap(),
            highest_finalized_cf: IntGauge::new("disabled", "disabled").unwrap(),
            highest_verified_cf: IntGauge::new("disabled", "disabled").unwrap(),
            syncing_peers: IntGauge::new("disabled", "disabled").unwrap(),
            events_synced: IntCounter::new("disabled", "disabled").unwrap(),
            cfs_synced: IntCounter::new("disabled", "disabled").unwrap(),
            sync_requests_sent: IntCounter::new("disabled", "disabled").unwrap(),
            sync_requests_received: IntCounter::new("disabled", "disabled").unwrap(),
            sync_latency: Histogram::with_opts(HistogramOpts::new("disabled", "disabled")).unwrap(),
            events_behind: IntGauge::new("disabled", "disabled").unwrap(),
            cfs_behind: IntGauge::new("disabled", "disabled").unwrap(),
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
