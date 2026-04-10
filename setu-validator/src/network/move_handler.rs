//! Move VM request handlers (Phase 4)
//!
//! Handles MoveCall and MovePublish HTTP submissions.
//! Follows the TransferHandler unit-struct pattern — all deps as function params.

use crate::InfraExecutor;
use crate::RouterManager;
use crate::TaskPreparer;
use super::tee_executor::TeeExecutor;
use setu_api::{MoveCallRequest, MoveCallResponse, MovePublishRequest, MovePublishResponse};
use setu_types::event::{Event, MoveCallPayload, VLCSnapshot};
use setu_types::object::ObjectId;
use setu_types::SubnetId;
use std::sync::Arc;
use tracing::{error, info, warn};

/// MoveCall handler — unit struct matching TransferHandler pattern
pub struct MoveCallHandler;

impl MoveCallHandler {
    /// Process a MoveCall submission
    ///
    /// Flow: convert request → Event → TaskPreparer.prepare_move_call_task()
    ///       → route to solver → TeeExecutor.execute_solver_inline_batch()
    ///       → spawn consensus → return result
    #[allow(clippy::too_many_arguments)]
    pub async fn submit_move_call(
        validator_id: &str,
        task_preparer: &TaskPreparer,
        router_manager: &RouterManager,
        tee_executor: &TeeExecutor,
        state_provider: &Arc<setu_storage::MerkleStateProvider>,
        vlc_time: u64,
        request: MoveCallRequest,
    ) -> MoveCallResponse {
        // 1. Convert MoveCallRequest → MoveCallPayload
        let mut payload = match Self::convert_request(&request) {
            Ok(p) => p,
            Err(e) => {
                return MoveCallResponse {
                    event_id: String::new(),
                    success: false,
                    state_changes: 0,
                    created_objects: vec![],
                    error: Some(e),
                };
            }
        };

        // 1.5. Auto-detect needs_tx_context from module bytecode
        //      Look up the target module from storage or embedded stdlib.
        {
            let module_key = format!("mod:{}::{}", payload.package, payload.module);
            let module_bytes = state_provider.get_raw_data(&module_key)
                .or_else(|| {
                    // Check embedded stdlib if target is at address 0x1
                    let stripped = payload.package.strip_prefix("0x").unwrap_or(&payload.package);
                    if stripped == "1" || stripped == "0000000000000000000000000000000000000000000000000000000000000001" {
                        setu_move_vm::engine::STDLIB_MODULES.iter()
                            .find(|(name, _)| *name == payload.module.as_str())
                            .map(|(_, bytes)| bytes.to_vec())
                    } else {
                        None
                    }
                });
            if let Some(bytes) = module_bytes {
                if let Some(detected) = setu_move_vm::engine::detect_needs_tx_context(&bytes, &payload.function) {
                    if detected != payload.needs_tx_context {
                        info!(
                            function = %payload.function,
                            declared = payload.needs_tx_context,
                            detected,
                            "Auto-detected needs_tx_context override"
                        );
                        payload.needs_tx_context = detected;
                    }
                }
            }
        }

        // 2. Build VLCSnapshot
        let vlc_snapshot = VLCSnapshot {
            vector_clock: setu_vlc::VectorClock::new(),
            logical_time: vlc_time,
            physical_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        // 3. Create ContractCall event
        let event = Event::move_call(
            payload.clone(),
            vec![],
            vlc_snapshot,
            validator_id.to_string(),
        );

        // 4. Determine subnet
        let subnet_id = match &request.subnet_id {
            Some(s) if s != "ROOT" => {
                warn!(subnet = %s, "Custom subnet not supported for MoveCall, using ROOT");
                SubnetId::ROOT
            }
            _ => SubnetId::ROOT,
        };

        // 5. Prepare SolverTask via TaskPreparer
        let solver_task = match task_preparer.prepare_move_call_task(&event, &payload, subnet_id) {
            Ok(task) => task,
            Err(e) => {
                error!(error = %e, "MoveCall task preparation failed");
                return MoveCallResponse {
                    event_id: String::new(),
                    success: false,
                    state_changes: 0,
                    created_objects: vec![],
                    error: Some(format!("Task preparation failed: {}", e)),
                };
            }
        };

        // 6. Route to solver
        let solver_id = match router_manager.route_any() {
            Ok(id) => id,
            Err(e) => {
                error!(error = %e, "No solver available for MoveCall");
                return MoveCallResponse {
                    event_id: String::new(),
                    success: false,
                    state_changes: 0,
                    created_objects: vec![],
                    error: Some(format!("No solver available: {}", e)),
                };
            }
        };

        // 7. Execute via TeeExecutor (no coin reservations needed for MoveCall)
        let call_id = format!("move-call-{}", vlc_time);
        match tee_executor.execute_solver_inline_batch(
            &call_id, &solver_id, solver_task, vec![],
        ).await {
            Ok((result_event, execution_time_us, events_processed)) => {
                let event_id = result_event.id.clone();
                let exec_result = result_event.execution_result.as_ref();
                let state_changes = exec_result
                    .map(|r| r.state_changes.len())
                    .unwrap_or(0);
                let success = exec_result
                    .map(|r| r.success)
                    .unwrap_or(false);
                let error = if success {
                    None
                } else {
                    exec_result.and_then(|r| r.message.clone())
                };

                // Debug: log all state change keys
                if let Some(r) = exec_result {
                    for sc in &r.state_changes {
                        info!(
                            key = %sc.key,
                            has_old = sc.old_value.is_some(),
                            has_new = sc.new_value.is_some(),
                            "MoveCall state_change entry"
                        );
                    }
                }

                // Extract created object keys from state changes
                // Created objects have new_value=Some but old_value=None, key starts with "oid:"
                let created_objects: Vec<String> = exec_result
                    .map(|r| {
                        r.state_changes.iter()
                            .filter(|sc| sc.key.starts_with("oid:") && sc.new_value.is_some() && sc.old_value.is_none())
                            .map(|sc| sc.key.clone())
                            .collect()
                    })
                    .unwrap_or_default();

                // Apply state changes to validator state (so created objects are
                // available for subsequent MoveCall references)
                if success {
                    if let Some(r) = result_event.execution_result.as_ref() {
                        let shared = state_provider.shared_state_manager();
                        let mut manager = shared.lock_write();
                        for sc in &r.state_changes {
                            manager.apply_state_change(SubnetId::ROOT, sc);
                        }
                        shared.publish_snapshot(&manager);
                    }
                }

                // Spawn consensus submission
                tee_executor.spawn_post_execution(
                    call_id, result_event, execution_time_us, events_processed,
                );

                info!(
                    event_id = %event_id,
                    state_changes,
                    created_objects = ?created_objects,
                    solver_id = %solver_id,
                    "MoveCall executed"
                );

                MoveCallResponse {
                    event_id,
                    success,
                    state_changes,
                    created_objects,
                    error,
                }
            }
            Err(e) => {
                error!(error = %e, "MoveCall TEE execution failed");
                MoveCallResponse {
                    event_id: String::new(),
                    success: false,
                    state_changes: 0,
                    created_objects: vec![],
                    error: Some(format!("Execution failed: {}", e)),
                }
            }
        }
    }

    /// Convert HTTP request to internal MoveCallPayload
    fn convert_request(request: &MoveCallRequest) -> Result<MoveCallPayload, String> {
        // Resolve sender to canonical hex address (handles both "alice" and "0x..." formats)
        let sender_hex = Self::resolve_address(&request.sender);

        // Decode hex args to raw bytes
        let args: Vec<Vec<u8>> = request.args.iter()
            .map(|hex_str| {
                hex::decode(hex_str.strip_prefix("0x").unwrap_or(hex_str))
                    .map_err(|e| format!("Invalid hex in arg: {}", e))
            })
            .collect::<Result<_, _>>()?;

        // Decode hex object IDs
        let input_object_ids: Vec<ObjectId> = request.input_object_ids.iter()
            .map(|hex_str| {
                ObjectId::from_hex(hex_str)
                    .map_err(|e| format!("Invalid ObjectId '{}': {}", hex_str, e))
            })
            .collect::<Result<_, _>>()?;

        Ok(MoveCallPayload {
            sender: sender_hex,
            package: request.package.clone(),
            module: request.module.clone(),
            function: request.function.clone(),
            type_args: request.type_args.clone(),
            args,
            input_object_ids,
            shared_object_ids: vec![],
            mutable_indices: if request.mutable_indices.is_empty() { None } else { Some(request.mutable_indices.clone()) },
            consumed_indices: if request.consumed_indices.is_empty() { None } else { Some(request.consumed_indices.clone()) },
            needs_tx_context: request.needs_tx_context,
        })
    }

    /// Resolve a human-readable name or hex string to a canonical hex address.
    /// Names like "alice" are hashed via blake3 to produce a deterministic address.
    fn resolve_address(name: &str) -> String {
        let stripped = name.strip_prefix("0x").unwrap_or(name);
        if stripped.len() == 64 && stripped.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!("0x{}", stripped);
        }
        let hash = blake3::hash(name.as_bytes());
        format!("0x{}", hex::encode(hash.as_bytes()))
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
