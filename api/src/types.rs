//! API-specific types
//!
//! Types used by the HTTP API layer that are not part of the core RPC protocol.

use serde::{Deserialize, Serialize};
use setu_types::event::{DynamicFieldAccess, Event};

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
    /// Input object IDs (hex-encoded ObjectIds) — must be owned by sender (or Immutable)
    #[serde(default)]
    pub input_object_ids: Vec<String>,
    /// Shared object IDs (hex-encoded ObjectIds) — must have `Ownership::Shared` (PWOO)
    #[serde(default)]
    pub shared_object_ids: Vec<String>,
    /// Indices into the concatenated `[input_object_ids..., shared_object_ids...]`
    /// list that are mutably borrowed.
    #[serde(default)]
    pub mutable_indices: Vec<usize>,
    /// Indices into `input_object_ids` that are consumed (transferred/deleted).
    /// Must be < `input_object_ids.len()` — shared objects cannot be consumed in PWOO Phase 1.
    #[serde(default)]
    pub consumed_indices: Vec<usize>,
    /// Whether the function needs TxContext injection
    #[serde(default = "default_true")]
    pub needs_tx_context: bool,
    /// Target subnet (defaults to ROOT)
    #[serde(default)]
    pub subnet_id: Option<String>,
    /// Dynamic-field access declarations (DF FDP M5).
    /// Each entry declares a (parent, key, mode) triple so the validator
    /// can preload the DF envelope + Merkle proof before TEE execution.
    /// Empty by default — pre-M5 JSON clients keep working unchanged.
    #[serde(default)]
    pub dynamic_field_accesses: Vec<DynamicFieldAccess>,
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
    /// Created object IDs (hex-encoded, "oid:{hex}" keys from state changes)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created_objects: Vec<String>,
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

// ============================================
// Move Object/Module Query Types (Phase 5b)
// ============================================

/// Response for Move object query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMoveObjectResponse {
    /// Object key ("oid:{hex}")
    pub key: String,
    /// Object ID (hex)
    pub object_id: String,
    /// Owner address (hex)
    pub owner: String,
    /// Ownership type
    pub ownership: String,
    /// Move type tag (e.g. "0x1::coin::Coin<0x1::setu::SETU>")
    pub type_tag: String,
    /// Object version
    pub version: u64,
    /// BCS-serialized data (hex)
    pub data_hex: String,
    /// Whether the object exists
    pub exists: bool,
    /// Error message (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for module ABI query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModuleAbiResponse {
    /// Module address
    pub address: String,
    /// Module name
    pub name: String,
    /// Public/entry functions
    pub functions: Vec<FunctionAbi>,
    /// Whether the module exists
    pub exists: bool,
    /// Error message (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Function ABI info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAbi {
    /// Function name
    pub name: String,
    /// Number of type parameters
    pub type_param_count: usize,
    /// Parameter types (string representation)
    pub parameters: Vec<String>,
    /// Whether this is an entry function
    pub is_entry: bool,
}

/// Response for listing modules at an address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListModulesResponse {
    /// Module address
    pub address: String,
    /// Module names
    pub modules: Vec<String>,
    /// Error message (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ============================================
// R5 · Event lookup (GET /api/v1/event/:id)
// ============================================

/// Chain-level verdict projection exposed to HTTP clients.
///
/// Distinct from `setu_types::ExecutionOutcome` so the API can evolve its
/// wire shape (e.g. add fields, rename) without touching consensus/storage
/// layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum OnChainOutcome {
    Applied {
        cf_id: String,
    },
    ExecutionFailed {
        cf_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    StaleRead {
        cf_id: String,
        conflicting_object: String,
        retry_hint: String,
    },
}

/// Off-chain (TEE / solver) execution result snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub state_changes_count: usize,
}

/// Minimal event metadata for client display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    pub event_type: String,
    pub creator: String,
    pub timestamp: u64,
    pub vlc_time: u64,
    pub parent_count: usize,
}

/// Response for `GET /api/v1/event/:id`.
///
/// `on_chain` is `None` until the event's CF is finalized by the local node.
/// `execution` is `None` for events that never reached the solver (e.g.
/// Genesis / validator-executed system events).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetEventResponse {
    pub event_id: String,
    pub status: String,
    pub execution: Option<ExecutionReport>,
    pub on_chain: Option<OnChainOutcome>,
    pub metadata: EventMetadata,
}

// ============================================
// M5-Pre tests — MoveCallRequest.dynamic_field_accesses
// ============================================

#[cfg(test)]
mod m5_pre_tests {
    use super::*;
    use setu_types::dynamic_field::DfAccessMode;

    /// Pre-M5 clients that omit `dynamic_field_accesses` must still deserialize;
    /// field defaults to an empty Vec. This guarantees wire back-compat.
    #[test]
    fn move_call_request_defaults_df_empty_when_missing() {
        let json = r#"{
            "sender": "alice",
            "package": "0xcafe",
            "module": "m",
            "function": "f"
        }"#;
        let req: MoveCallRequest = serde_json::from_str(json).unwrap();
        assert!(req.dynamic_field_accesses.is_empty());
        // Sanity: other defaulted fields unaffected.
        assert!(req.needs_tx_context);
    }

    /// Serialize → deserialize preserves every DF access entry end-to-end.
    #[test]
    fn move_call_request_df_roundtrip() {
        let orig = MoveCallRequest {
            sender: "alice".to_string(),
            package: "0xcafe".to_string(),
            module: "df_registry".to_string(),
            function: "put_u64".to_string(),
            type_args: vec![],
            args: vec![],
            input_object_ids: vec!["aa".repeat(32)],
            shared_object_ids: vec![],
            mutable_indices: vec![0],
            consumed_indices: vec![],
            needs_tx_context: false,
            subnet_id: None,
            dynamic_field_accesses: vec![DynamicFieldAccess {
                parent_object_id: format!("oid:{}", "aa".repeat(32)),
                key_type: "u64".to_string(),
                key_bcs_hex: "0100000000000000".to_string(),
                mode: DfAccessMode::Create,
                value_type: Some("u64".to_string()),
            }],
        };
        let json = serde_json::to_string(&orig).unwrap();
        let back: MoveCallRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dynamic_field_accesses.len(), 1);
        assert_eq!(back.dynamic_field_accesses[0].mode, DfAccessMode::Create);
        assert_eq!(
            back.dynamic_field_accesses[0].value_type.as_deref(),
            Some("u64")
        );
    }
}
