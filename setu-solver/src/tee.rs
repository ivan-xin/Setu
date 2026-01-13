//! TEE (Trusted Execution Environment) integration for Solver
//!
//! This module provides a wrapper around `setu-enclave` for Solver-specific use cases.
//! It bridges the gap between the generic enclave abstraction and the Solver's needs.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                          Solver                                     │
//! │  ┌───────────────────────────────────────────────────────────────┐  │
//! │  │                   TeeExecutor (this module)                    │  │
//! │  │   • Wraps EnclaveRuntime                                       │  │
//! │  │   • Converts Event/Transfer → StfInput                         │  │
//! │  │   • Handles attestation generation                             │  │
//! │  └───────────────────────────────────────────────────────────────┘  │
//! │                              │                                      │
//! │                              ▼                                      │
//! │  ┌───────────────────────────────────────────────────────────────┐  │
//! │  │                setu-enclave (EnclaveRuntime)                   │  │
//! │  │   • MockEnclave (dev/test)                                     │  │
//! │  │   • NitroEnclave (production)                                  │  │
//! │  └───────────────────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```

use core_types::Transfer;
use setu_enclave::{
    Attestation, EnclaveInfo, EnclaveRuntime,
    MockEnclave, StfInput, StfOutput, ReadSetEntry,
};
use setu_types::event::{Event, ExecutionResult, StateChange};
use setu_types::SubnetId;
use std::sync::Arc;
use tracing::{info, debug};

/// TEE Executor for Solver nodes
///
/// Wraps an `EnclaveRuntime` implementation and provides high-level APIs
/// for event execution and attestation generation.
pub struct TeeExecutor {
    solver_id: String,
    enclave: Arc<dyn EnclaveRuntime>,
}

impl TeeExecutor {
    /// Create a new TEE Executor with the default enclave (MockEnclave for now)
    pub fn new(solver_id: String) -> Self {
        let enclave = MockEnclave::default_with_solver_id(solver_id.clone());
        
        info!(
            solver_id = %solver_id,
            platform = %enclave.info().platform,
            "Initializing TEE Executor"
        );
        
        Self {
            solver_id,
            enclave: Arc::new(enclave),
        }
    }
    
    /// Create with a custom enclave implementation
    pub fn with_enclave(solver_id: String, enclave: Arc<dyn EnclaveRuntime>) -> Self {
        info!(
            solver_id = %solver_id,
            platform = %enclave.info().platform,
            simulated = enclave.is_simulated(),
            "Initializing TEE Executor with custom enclave"
        );
        
        Self {
            solver_id,
            enclave,
        }
    }
    
    /// Execute events and generate attestation
    ///
    /// This is the main entry point for Solver event execution.
    pub async fn execute_events(
        &self,
        subnet_id: SubnetId,
        pre_state_root: [u8; 32],
        events: Vec<Event>,
        read_set: Vec<(String, Vec<u8>)>,
    ) -> anyhow::Result<TeeExecutionResult> {
        debug!(
            solver_id = %self.solver_id,
            subnet_id = ?subnet_id,
            event_count = events.len(),
            "Executing events in TEE"
        );
        
        // Build StfInput
        let input = StfInput::new(subnet_id.clone(), pre_state_root)
            .with_events(events)
            .with_read_set(
                read_set.into_iter()
                    .map(|(k, v)| ReadSetEntry::new(k, v))
                    .collect()
            );
        
        // Execute STF
        let output = self.enclave.execute_stf(input).await
            .map_err(|e| anyhow::anyhow!("STF execution failed: {}", e))?;
        
        info!(
            solver_id = %self.solver_id,
            events_processed = output.events_processed.len(),
            events_failed = output.events_failed.len(),
            "TEE execution completed"
        );
        
        Ok(TeeExecutionResult::from_stf_output(output))
    }
    
    /// Execute a single transfer (legacy API for backward compatibility)
    #[allow(dead_code)]
    pub async fn execute_transfer(
        &self,
        transfer: &Transfer,
        pre_state_root: [u8; 32],
    ) -> anyhow::Result<TeeExecutionResult> {
        // Convert transfer to event (simplified)
        let event = self.transfer_to_event(transfer)?;
        
        // Get subnet_id from transfer or default to ROOT
        let subnet_id = transfer.subnet_id
            .as_ref()
            .map(|s| SubnetId::from_str_id(s))
            .unwrap_or(SubnetId::ROOT);
        
        // Build read set from transfer
        let read_set = vec![
            (format!("balance:{}", transfer.from), vec![]),
            (format!("balance:{}", transfer.to), vec![]),
        ];
        
        self.execute_events(subnet_id, pre_state_root, vec![event], read_set).await
    }
    
    /// Get enclave information
    pub fn enclave_info(&self) -> EnclaveInfo {
        self.enclave.info()
    }
    
    /// Check if running in simulated mode
    pub fn is_simulated(&self) -> bool {
        self.enclave.is_simulated()
    }
    
    /// Get enclave measurement
    #[allow(dead_code)]
    pub fn measurement(&self) -> [u8; 32] {
        self.enclave.measurement()
    }
    
    // Helper: Convert Transfer to Event
    fn transfer_to_event(&self, _transfer: &Transfer) -> anyhow::Result<Event> {
        use setu_types::event::{EventType, VLCSnapshot};
        
        Ok(Event::new(
            EventType::Transfer,
            vec![],
            VLCSnapshot::default(),
            self.solver_id.clone(),
        ))
    }
}

/// Result of TEE execution
#[derive(Debug, Clone)]
pub struct TeeExecutionResult {
    /// Subnet that was executed
    pub subnet_id: SubnetId,
    /// Post-execution state root
    pub post_state_root: [u8; 32],
    /// State changes to apply
    pub state_changes: Vec<StateChange>,
    /// Number of events processed
    pub events_processed: usize,
    /// Number of events failed
    pub events_failed: usize,
    /// TEE attestation
    pub attestation: Attestation,
    /// Execution time in microseconds
    pub execution_time_us: u64,
}

impl TeeExecutionResult {
    /// Convert from StfOutput
    pub fn from_stf_output(output: StfOutput) -> Self {
        // Convert StateDiff to Vec<StateChange>
        let state_changes: Vec<StateChange> = output.state_diff.writes
            .into_iter()
            .map(|w| StateChange {
                key: w.key,
                old_value: w.old_value,
                new_value: Some(w.new_value),
            })
            .chain(
                output.state_diff.deletes.into_iter().map(|k| StateChange {
                    key: k,
                    old_value: None,
                    new_value: None,
                })
            )
            .collect();
        
        Self {
            subnet_id: output.subnet_id,
            post_state_root: output.post_state_root,
            state_changes,
            events_processed: output.events_processed.len(),
            events_failed: output.events_failed.len(),
            attestation: output.attestation,
            execution_time_us: output.stats.execution_time_us,
        }
    }
    
    /// Convert to ExecutionResult (for backward compatibility)
    pub fn to_execution_result(&self) -> ExecutionResult {
        ExecutionResult {
            success: self.events_failed == 0,
            message: if self.events_failed == 0 {
                Some(format!("Processed {} events", self.events_processed))
            } else {
                Some(format!("{} events failed", self.events_failed))
            },
            state_changes: self.state_changes.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use setu_enclave::EnclavePlatform;
    use setu_types::event::{EventType, VLCSnapshot};
    
    #[tokio::test]
    async fn test_tee_executor_creation() {
        let executor = TeeExecutor::new("test-solver".to_string());
        let info = executor.enclave_info();
        
        assert_eq!(info.platform, EnclavePlatform::Mock);
        assert!(executor.is_simulated());
    }
    
    #[tokio::test]
    async fn test_execute_events() {
        let executor = TeeExecutor::new("test-solver".to_string());
        
        let event = Event::new(
            EventType::Transfer,
            vec![],
            VLCSnapshot::default(),
            "test".to_string(),
        );
        
        let result = executor.execute_events(
            SubnetId::ROOT,
            [0u8; 32],
            vec![event],
            vec![],
        ).await;
        
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.events_processed, 1);
        assert!(result.attestation.is_mock());
    }
    
    #[tokio::test]
    async fn test_execution_result_conversion() {
        let executor = TeeExecutor::new("test-solver".to_string());
        
        let event = Event::new(
            EventType::Transfer,
            vec![],
            VLCSnapshot::default(),
            "test".to_string(),
        );
        
        let result = executor.execute_events(
            SubnetId::ROOT,
            [0u8; 32],
            vec![event],
            vec![],
        ).await.unwrap();
        
        let exec_result = result.to_execution_result();
        assert!(exec_result.success);
    }
}

