// Copyright (c) Setu Contributors
// SPDX-License-Identifier: Apache-2.0

//! Network constants

use std::time::Duration;

/// Default maximum frame size (8MB)
pub const MAX_FRAME_SIZE: usize = 8 * 1024 * 1024;

/// Default maximum message size (4MB)
pub const MAX_MESSAGE_SIZE: usize = 4 * 1024 * 1024;

/// Default channel buffer size
pub const CHANNEL_SIZE: usize = 1024;

/// Default connection timeout
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Default request timeout
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Default heartbeat interval
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Default peer discovery interval
pub const DISCOVERY_INTERVAL: Duration = Duration::from_secs(60);

/// Maximum number of connection retries
pub const MAX_CONNECTION_RETRIES: usize = 5;

/// Connection retry backoff base (exponential)
pub const RETRY_BACKOFF_BASE: Duration = Duration::from_secs(1);

/// Maximum connection retry backoff
pub const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(60);

/// Default port for validators
pub const DEFAULT_VALIDATOR_PORT: u16 = 9000;

/// Default port for solvers
pub const DEFAULT_SOLVER_PORT: u16 = 9001;

/// Maximum pending connections
pub const MAX_PENDING_CONNECTIONS: usize = 100;

/// Maximum idle time before disconnecting
pub const MAX_IDLE_TIME: Duration = Duration::from_secs(300); // 5 minutes

/// Network protocol version
pub const NETWORK_PROTOCOL_VERSION: u32 = 1;
