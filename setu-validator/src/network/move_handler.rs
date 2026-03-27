//! Move VM request handlers (Phase 4)
//!
//! Handles MoveCall and MovePublish HTTP submissions.
//! Follows the TransferHandler unit-struct pattern — all deps as function params.

use crate::InfraExecutor;
use setu_api::{MoveCallRequest, MoveCallResponse, MovePublishRequest, MovePublishResponse};
use setu_types::event::{Event, VLCSnapshot};
use tracing::{info, warn};

/// MoveCall handler — unit struct matching TransferHandler pattern
pub struct MoveCallHandler;

impl MoveCallHandler {
    /// Process a MoveCall submission
    ///
    /// Phase 4 stub: Currently returns a placeholder since the full
    /// TaskPreparer → Solver → Enclave pipeline requires prepare_move_call_task
    /// and solver routing which will be wired in a follow-up PR once
    /// the TaskPreparer MoveCall path is integrated with TeeExecutor.
    #[allow(clippy::too_many_arguments)]
    pub async fn submit_move_call(
        _request: MoveCallRequest,
    ) -> MoveCallResponse {
        // TODO(phase4-followup): Wire TaskPreparer.prepare_move_call_task() → Solver → Enclave
        // For now, return not-yet-implemented so the API exists and is routable
        MoveCallResponse {
            event_id: String::new(),
            success: false,
            state_changes: 0,
            error: Some("MoveCall execution pipeline not yet wired (Phase 4 P2)".into()),
        }
    }
}

/// MovePublish handler — unit struct matching TransferHandler pattern
pub struct MovePublishHandler;

impl MovePublishHandler {
    /// Process a ContractPublish submission
    ///
    /// Flow: decode hex modules → InfraExecutor.execute_contract_publish() → return (response, event)
    pub async fn submit_move_publish(
        infra_executor: &InfraExecutor,
        vlc_time: u64,
        request: MovePublishRequest,
    ) -> (MovePublishResponse, Option<Event>) {
        // 1. Validate & decode modules from hex
        if request.modules.is_empty() {
            return (MovePublishResponse {
                event_id: String::new(),
                module_count: 0,
                success: false,
                error: Some("Empty module list".into()),
            }, None);
        }

        let modules_bytes: Vec<Vec<u8>> = match request.modules.iter()
            .map(|hex_str| hex::decode(hex_str.strip_prefix("0x").unwrap_or(hex_str))
                .map_err(|e| format!("Invalid hex in module: {}", e)))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(m) => m,
            Err(e) => {
                return (MovePublishResponse {
                    event_id: String::new(),
                    module_count: 0,
                    success: false,
                    error: Some(e),
                }, None);
            }
        };

        // 2. Build VLCSnapshot
        let vlc_snapshot = VLCSnapshot {
            vector_clock: setu_vlc::VectorClock::new(),
            logical_time: vlc_time,
            physical_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        // 3. Execute via InfraExecutor
        let module_count = modules_bytes.len();
        match infra_executor.execute_contract_publish(&request.sender, &modules_bytes, vlc_snapshot) {
            Ok(event) => {
                let event_id = event.id.clone();
                info!(event_id = %event_id, module_count, "MovePublish executed successfully");
                (MovePublishResponse {
                    event_id,
                    module_count,
                    success: true,
                    error: None,
                }, Some(event))
            }
            Err(e) => {
                warn!(error = %e, "MovePublish execution failed");
                (MovePublishResponse {
                    event_id: String::new(),
                    module_count: 0,
                    success: false,
                    error: Some(e),
                }, None)
            }
        }
    }
}
