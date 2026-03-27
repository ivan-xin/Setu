//! API-specific types
//!
//! Types used by the HTTP API layer that are not part of the core RPC protocol.

use serde::{Deserialize, Serialize};
use setu_types::event::Event;

// ============================================
// Event Submission
// ============================================

/// Request to submit an event to the validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitEventRequest {
    /// The event to submit
    pub event: Event,
}

/// Response to event submission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitEventResponse {
    /// Whether submission was successful
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// Event ID
    pub event_id: Option<String>,
    /// VLC time assigned
    pub vlc_time: Option<u64>,
}

// ============================================
// State Query Types (Scheme B)
// ============================================

/// Response for balance query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBalanceResponse {
    /// Account address
    pub account: String,
    /// Balance amount
    pub balance: u128,
    /// Whether the account exists
    pub exists: bool,
}

/// Response for object query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetObjectResponse {
    /// Object key
    pub key: String,
    /// Object value (if exists)
    pub value: Option<Vec<u8>>,
    /// Whether the object exists
    pub exists: bool,
}

// ============================================
// Move VM Types (Phase 4)
// ============================================

/// Request to call a Move function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveCallRequest {
    /// Transaction sender address (hex)
    pub sender: String,
    /// Package address (hex) where the module lives
    pub package: String,
    /// Module name
    pub module: String,
    /// Function name
    pub function: String,
    /// Type arguments (string representation)
    #[serde(default)]
    pub type_args: Vec<String>,
    /// Pure arguments (hex-encoded BCS bytes)
    #[serde(default)]
    pub args: Vec<String>,
    /// Input object IDs (hex-encoded ObjectIds)
    #[serde(default)]
    pub input_object_ids: Vec<String>,
    /// Indices into input_object_ids that are mutably borrowed
    #[serde(default)]
    pub mutable_indices: Vec<usize>,
    /// Indices into input_object_ids that are consumed (transferred/deleted)
    #[serde(default)]
    pub consumed_indices: Vec<usize>,
    /// Whether the function needs TxContext injection
    #[serde(default = "default_true")]
    pub needs_tx_context: bool,
    /// Target subnet (defaults to ROOT)
    #[serde(default)]
    pub subnet_id: Option<String>,
}

/// Response to Move function call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveCallResponse {
    /// Event ID of the submitted event
    pub event_id: String,
    /// Whether execution succeeded
    pub success: bool,
    /// Number of state changes produced
    pub state_changes: usize,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Request to publish Move modules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovePublishRequest {
    /// Publisher address (hex)
    pub sender: String,
    /// Compiled module bytecodes (hex-encoded)
    pub modules: Vec<String>,
}

/// Response to Move module publish
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovePublishResponse {
    /// Event ID of the submitted event
    pub event_id: String,
    /// Number of modules published
    pub module_count: usize,
    /// Whether publish succeeded
    pub success: bool,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn default_true() -> bool {
    true
}
