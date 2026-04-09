//! Infrastructure Event Executor
//!
//! Executes infrastructure events (SubnetRegister, UserRegister) directly in Validator.
//! These events are NOT routed to Solvers/TEE because they are core infrastructure
//! operations managed by validators.
//!
//! ## Architecture
//!
//! ```text
//! Infrastructure Events (Validator-executed):
//! - SubnetRegister: Create subnet, mint initial tokens
//! - UserRegister: Register user membership
//! - ValidatorRegister/Unregister
//! - SolverRegister/Unregister
//!
//! Application Events (Solver-executed via TEE):
//! - Transfer: Token transfers
//! - (Future) Smart contract calls
//! ```
//!
//! ## Design Philosophy
//!
//! Infrastructure primitives (subnet/user registration) are handled by Validator
//! to ensure consistency and avoid TEE complexity for non-economic operations.
//! Token operations (initial minting, airdrops) use the same RuntimeExecutor
//! logic as TEE to maintain consistency.

use setu_runtime::{RuntimeExecutor, ExecutionContext, ExecutionOutput, InMemoryStateStore};
use setu_storage::{SharedStateManager, MerkleStateProvider};
use setu_types::{
    Address, SubnetId,
    registration::{SubnetRegistration, UserRegistration},
    event::{Event, EventPayload, ExecutionResult, StateChange as EventStateChange},
};
use setu_vlc::VLCSnapshot;
use std::sync::Arc;
use tracing::{info, warn, error};

/// Infrastructure event executor for Validator
///
/// Executes SubnetRegister, UserRegister events directly without TEE.
pub struct InfraExecutor {
    /// Validator ID
    validator_id: String,
    /// State provider for reading/writing state
    state_provider: Arc<MerkleStateProvider>,
}

impl InfraExecutor {
    /// Create a new infrastructure executor
    pub fn new(validator_id: String, state_provider: Arc<MerkleStateProvider>) -> Self {
        Self {
            validator_id,
            state_provider,
        }
    }

    /// Execute a SubnetRegister event
    ///
    /// This creates a subnet and optionally mints initial tokens to the owner.
    pub fn execute_subnet_register(
        &self,
        registration: &SubnetRegistration,
        vlc_snapshot: VLCSnapshot,
    ) -> Result<Event, String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis() as u64;

        // Derive tx_hash from registration for deterministic ID generation
        let tx_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"SETU_TX_HASH:VALIDATOR:SUBNET:");
            hasher.update(registration.subnet_id.as_bytes());
            hasher.update(&timestamp.to_le_bytes());
            *hasher.finalize().as_bytes()
        };
        let ctx = ExecutionContext::new(
            self.validator_id.clone(),
            timestamp,
            false,
            tx_hash,
        );

        // Create a temporary InMemoryStateStore for execution
        // In production, this would integrate with the actual state
        let temp_store = InMemoryStateStore::new();
        let mut runtime = RuntimeExecutor::new(temp_store);

        let owner = Address::from_hex(&registration.owner)
            .map_err(|e| format!("Invalid owner address '{}': {}", registration.owner, e))?;

        let output = runtime.execute_subnet_register(
            &registration.subnet_id,
            &registration.name,
            &owner,
            registration.token_symbol.as_deref(),
            registration.initial_token_supply,
            &ctx,
        ).map_err(|e| format!("Runtime error: {}", e))?;

        if !output.success {
            return Err(output.message.unwrap_or_else(|| "Subnet registration failed".to_string()));
        }

        // Convert RuntimeExecutor output to Event
        let mut event = Event::subnet_register(
            registration.clone(),
            vec![], // No parent events
            vlc_snapshot,
            self.validator_id.clone(),
        );

        // Apply state changes to the actual state provider first
        self.apply_state_changes(&output)?;

        // Set execution result — use canonical "oid:{hex}" key format
        let state_changes: Vec<EventStateChange> = output.state_changes.iter()
            .map(|sc| sc.to_event_state_change())
            .collect();

        event.set_execution_result(ExecutionResult {
            success: true,
            message: output.message,
            state_changes,
        });

        info!(
            subnet_id = %registration.subnet_id,
            owner = %registration.owner,
            token_symbol = ?registration.token_symbol,
            initial_supply = ?registration.initial_token_supply,
            event_id = %event.id,
            "Subnet registered by Validator"
        );

        Ok(event)
    }

    /// Execute a UserRegister event
    ///
    /// This registers a user in a subnet (membership only, no automatic airdrop).
    pub fn execute_user_register(
        &self,
        registration: &UserRegistration,
        vlc_snapshot: VLCSnapshot,
    ) -> Result<Event, String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis() as u64;

        // Derive tx_hash from registration for deterministic ID generation
        let tx_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"SETU_TX_HASH:VALIDATOR:USER:");
            hasher.update(registration.address.as_bytes());
            hasher.update(&timestamp.to_le_bytes());
            *hasher.finalize().as_bytes()
        };
        let ctx = ExecutionContext::new(
            self.validator_id.clone(),
            timestamp,
            false,
            tx_hash,
        );

        let temp_store = InMemoryStateStore::new();
        let mut runtime = RuntimeExecutor::new(temp_store);

        let user_address = Address::from_hex(&registration.address)
            .map_err(|e| format!("Invalid user address '{}': {}", registration.address, e))?;
        let subnet_id = registration.subnet_id.as_deref().unwrap_or("subnet-0");

        let output = runtime.execute_user_register(
            &user_address,
            subnet_id,
            &ctx,
        ).map_err(|e| format!("Runtime error: {}", e))?;

        if !output.success {
            return Err(output.message.unwrap_or_else(|| "User registration failed".to_string()));
        }

        // Convert to Event
        let mut event = Event::user_register(
            registration.clone(),
            vec![],
            vlc_snapshot,
            self.validator_id.clone(),
        );

        // Apply state changes first
        self.apply_state_changes(&output)?;

        // Use canonical "oid:{hex}" key format
        let state_changes: Vec<EventStateChange> = output.state_changes.iter()
            .map(|sc| sc.to_event_state_change())
            .collect();

        event.set_execution_result(ExecutionResult {
            success: true,
            message: output.message,
            state_changes,
        });

        info!(
            user = %registration.address,
            subnet_id = %subnet_id,
            event_id = %event.id,
            "User registered by Validator"
        );

        Ok(event)
    }

    // ========== Phase 4: Move VM Contract Publish ==========

    /// Execute a ContractPublish event (Move module deployment)
    ///
    /// Verifies Move bytecode and stores modules in ROOT subnet SMT.
    /// Follows the same pattern as execute_subnet_register():
    /// execute → eager apply state → set execution_result → return Event
    pub fn execute_contract_publish(
        &self,
        sender: &str,
        modules_bytes: &[Vec<u8>],
        vlc_snapshot: VLCSnapshot,
    ) -> Result<Event, String> {
        use move_binary_format::CompiledModule;
        use move_bytecode_verifier::verify_module_unmetered;

        if modules_bytes.is_empty() {
            return Err("Empty module list".into());
        }

        let mut state_changes: Vec<EventStateChange> = Vec::new();

        for module_bytes in modules_bytes {
            // 1. Deserialize
            let compiled = CompiledModule::deserialize_with_defaults(module_bytes)
                .map_err(|e| format!("Module deserialization failed: {}", e))?;

            // 2. Bytecode verification
            verify_module_unmetered(&compiled)
                .map_err(|e| format!("Bytecode verification failed: {}", e))?;

            // 3. Build "mod:{addr}::{name}" key
            let module_addr = compiled.self_id().address().to_hex_literal();
            let module_name = compiled.self_id().name().to_string();
            let module_key = format!("mod:{}::{}", module_addr, module_name);

            // 4. ADR-4: reject duplicate publish — check on-chain state
            if self.state_provider.get_raw_data(&module_key).is_some() {
                return Err(format!("Module already published (ADR-4): {}", module_key));
            }

            // 5. In-batch duplicate check
            if state_changes.iter().any(|sc| sc.key == module_key) {
                return Err(format!("Duplicate module in batch: {}", module_key));
            }

            // 6. Build StateChange (key = "mod:{addr}::{name}", value = raw bytecode)
            state_changes.push(EventStateChange::insert(
                module_key,
                module_bytes.clone(),
            ));
        }

        // 7. Eagerly apply state changes to MerkleStateProvider
        {
            let shared = self.state_provider.shared_state_manager();
            let mut manager = shared.lock_write();
            for sc in &state_changes {
                manager.apply_state_change(SubnetId::ROOT, sc);
            }
            shared.publish_snapshot(&manager);
        }

        // 8. Build Event
        let mut event = Event::contract_publish(
            sender.to_string(),
            modules_bytes.to_vec(),
            vec![], // No parent events
            vlc_snapshot,
            self.validator_id.clone(),
        );

        // 9. Set execution_result
        event.set_execution_result(ExecutionResult {
            success: true,
            message: Some(format!("{} module(s) published", state_changes.len())),
            state_changes,
        });

        info!(
            sender = %sender,
            module_count = modules_bytes.len(),
            event_id = %event.id,
            "Contract published by Validator"
        );

        Ok(event)
    }

    /// Apply state changes to the MerkleStateProvider
    ///
    /// Routes through `apply_state_change()` which handles both SMT updates
    /// AND coin index updates (coin_type_index, owner_coin_index).
    fn apply_state_changes(&self, output: &ExecutionOutput) -> Result<(), String> {
        // Get write access to the state manager
        let shared = self.state_provider.shared_state_manager();
        let mut manager = shared.lock_write();

        for sc in &output.state_changes {
            // Convert to event-layer StateChange with canonical "oid:{hex}" key format
            // and route through apply_state_change for proper SMT + index updates
            let event_sc = sc.to_event_state_change();
            manager.apply_state_change(SubnetId::ROOT, &event_sc);
        }
        
        // Publish snapshot so read path sees the infra changes immediately
        shared.publish_snapshot(&manager);

        Ok(())
    }

    // ========== Phase 3: Profile & Subnet Membership ==========

    /// Execute a profile update event
    pub fn execute_profile_update(
        &self,
        user_address: &str,
        display_name: Option<&str>,
        avatar_url: Option<&str>,
        bio: Option<&str>,
        attributes: &std::collections::HashMap<String, String>,
        vlc_snapshot: VLCSnapshot,
    ) -> Result<Event, String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis() as u64;

        let tx_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"SETU_TX_HASH:VALIDATOR:PROFILE:");
            hasher.update(user_address.as_bytes());
            hasher.update(&timestamp.to_le_bytes());
            *hasher.finalize().as_bytes()
        };
        let ctx = ExecutionContext::new(
            self.validator_id.clone(), timestamp, false, tx_hash,
        );

        let temp_store = InMemoryStateStore::new();
        let mut runtime = RuntimeExecutor::new(temp_store);
        let address = Address::from_hex(user_address)
            .map_err(|e| format!("Invalid address '{}': {}", user_address, e))?;

        let output = runtime.execute_profile_update(
            &address, display_name, avatar_url, bio, attributes, &ctx,
        ).map_err(|e| format!("Runtime error: {}", e))?;

        if !output.success {
            return Err(output.message.unwrap_or_else(|| "Profile update failed".to_string()));
        }

        let mut event = Event::new(
            setu_types::event::EventType::System, vec![], vlc_snapshot, self.validator_id.clone(),
        );

        self.apply_state_changes(&output)?;

        let state_changes: Vec<EventStateChange> = output.state_changes.iter()
            .map(|sc| sc.to_event_state_change())
            .collect();
        event.set_execution_result(ExecutionResult {
            success: true,
            message: output.message,
            state_changes,
        });

        info!(user = %user_address, event_id = %event.id, "Profile updated by Validator");
        Ok(event)
    }

    /// Execute a subnet join event
    pub fn execute_subnet_join(
        &self,
        user_address: &str,
        subnet_id: &str,
        vlc_snapshot: VLCSnapshot,
    ) -> Result<Event, String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis() as u64;

        let tx_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"SETU_TX_HASH:VALIDATOR:JOIN:");
            hasher.update(user_address.as_bytes());
            hasher.update(subnet_id.as_bytes());
            hasher.update(&timestamp.to_le_bytes());
            *hasher.finalize().as_bytes()
        };
        let ctx = ExecutionContext::new(
            self.validator_id.clone(), timestamp, false, tx_hash,
        );

        let temp_store = InMemoryStateStore::new();
        let mut runtime = RuntimeExecutor::new(temp_store);
        let address = Address::from_hex(user_address)
            .map_err(|e| format!("Invalid address '{}': {}", user_address, e))?;

        let output = runtime.execute_subnet_join(&address, subnet_id, &ctx)
            .map_err(|e| format!("Runtime error: {}", e))?;

        if !output.success {
            return Err(output.message.unwrap_or_else(|| "Subnet join failed".to_string()));
        }

        let mut event = Event::new(
            setu_types::event::EventType::System, vec![], vlc_snapshot, self.validator_id.clone(),
        );

        self.apply_state_changes(&output)?;

        let state_changes: Vec<EventStateChange> = output.state_changes.iter()
            .map(|sc| sc.to_event_state_change())
            .collect();
        event.set_execution_result(ExecutionResult {
            success: true,
            message: output.message,
            state_changes,
        });

        info!(user = %user_address, subnet_id = %subnet_id, event_id = %event.id, "Subnet join by Validator");
        Ok(event)
    }

    /// Execute a subnet leave event
    pub fn execute_subnet_leave(
        &self,
        user_address: &str,
        subnet_id: &str,
        vlc_snapshot: VLCSnapshot,
    ) -> Result<Event, String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis() as u64;

        let tx_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"SETU_TX_HASH:VALIDATOR:LEAVE:");
            hasher.update(user_address.as_bytes());
            hasher.update(subnet_id.as_bytes());
            hasher.update(&timestamp.to_le_bytes());
            *hasher.finalize().as_bytes()
        };
        let ctx = ExecutionContext::new(
            self.validator_id.clone(), timestamp, false, tx_hash,
        );

        let temp_store = InMemoryStateStore::new();
        let mut runtime = RuntimeExecutor::new(temp_store);
        let address = Address::from_hex(user_address)
            .map_err(|e| format!("Invalid address '{}': {}", user_address, e))?;

        let output = runtime.execute_subnet_leave(&address, subnet_id, &ctx)
            .map_err(|e| format!("Runtime error: {}", e))?;

        if !output.success {
            return Err(output.message.unwrap_or_else(|| "Subnet leave failed".to_string()));
        }

        let mut event = Event::new(
            setu_types::event::EventType::System, vec![], vlc_snapshot, self.validator_id.clone(),
        );

        self.apply_state_changes(&output)?;

        let state_changes: Vec<EventStateChange> = output.state_changes.iter()
            .map(|sc| sc.to_event_state_change())
            .collect();
        event.set_execution_result(ExecutionResult {
            success: true,
            message: output.message,
            state_changes,
        });

        info!(user = %user_address, subnet_id = %subnet_id, event_id = %event.id, "Subnet leave by Validator");
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use setu_storage::GlobalStateManager;

    #[test]
    fn test_subnet_register() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(shared));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);

        let registration = SubnetRegistration::new("subnet-test", "Test Subnet", "0xc0a6c424ac7157ae408398df7e5f4552091a69125d5dfcb7b8c2659029395bdf", "TEST")
            .with_initial_supply(1_000_000);

        let vlc = VLCSnapshot {
            vector_clock: setu_vlc::VectorClock::new(),
            logical_time: 1,
            physical_time: 1000,
        };

        let result = executor.execute_subnet_register(&registration, vlc);
        assert!(result.is_ok());
        
        let event = result.unwrap();
        assert!(event.execution_result.is_some());
        assert!(event.execution_result.as_ref().unwrap().success);
    }

    #[test]
    fn test_user_register() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(shared));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);

        let registration = UserRegistration::from_metamask(
            "0xc0a6c424ac7157ae408398df7e5f4552091a69125d5dfcb7b8c2659029395bdf",
            1000,
        ).with_subnet("subnet-0");

        let vlc = VLCSnapshot {
            vector_clock: setu_vlc::VectorClock::new(),
            logical_time: 1,
            physical_time: 1000,
        };

        let result = executor.execute_user_register(&registration, vlc);
        assert!(result.is_ok());
        
        let event = result.unwrap();
        assert!(event.execution_result.is_some());
        assert!(event.execution_result.as_ref().unwrap().success);
    }

    fn test_vlc() -> VLCSnapshot {
        VLCSnapshot {
            vector_clock: setu_vlc::VectorClock::new(),
            logical_time: 1,
            physical_time: 1000,
        }
    }

    #[test]
    fn test_infra_profile_update_produces_system_event() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(shared));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);
        let attrs = std::collections::HashMap::new();

        let result = executor.execute_profile_update(
            "0xc0a6c424ac7157ae408398df7e5f4552091a69125d5dfcb7b8c2659029395bdf",
            Some("Alice"), None, Some("Hello world"), &attrs, test_vlc(),
        );
        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.event_type, setu_types::event::EventType::System);
        let er = event.execution_result.as_ref().unwrap();
        assert!(er.success);
        assert_eq!(er.state_changes.len(), 1);
        // Key must be "oid:{hex}" format
        assert!(er.state_changes[0].key.starts_with("oid:"));
    }

    #[test]
    fn test_infra_subnet_join_produces_system_event() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(shared));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);

        let result = executor.execute_subnet_join(
            "0xc0a6c424ac7157ae408398df7e5f4552091a69125d5dfcb7b8c2659029395bdf",
            "defi-subnet", test_vlc(),
        );
        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.event_type, setu_types::event::EventType::System);
        let er = event.execution_result.as_ref().unwrap();
        assert!(er.success);
        assert_eq!(er.state_changes.len(), 2);
        assert!(er.state_changes[0].key.starts_with("oid:"));
        assert!(er.state_changes[1].key.starts_with("oid:"));
        // Both have new_value (Create)
        assert!(er.state_changes[0].new_value.is_some());
        assert!(er.state_changes[1].new_value.is_some());
    }

    #[test]
    fn test_infra_subnet_leave_produces_system_event() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(shared));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);

        let result = executor.execute_subnet_leave(
            "0xc0a6c424ac7157ae408398df7e5f4552091a69125d5dfcb7b8c2659029395bdf",
            "defi-subnet", test_vlc(),
        );
        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.event_type, setu_types::event::EventType::System);
        let er = event.execution_result.as_ref().unwrap();
        assert!(er.success);
        assert_eq!(er.state_changes.len(), 2);
        // Delete: new_value = None
        assert!(er.state_changes[0].new_value.is_none());
        assert!(er.state_changes[1].new_value.is_none());
    }

    /// Helper: create a valid Move module with a given address and name
    fn make_module_bytes(addr: move_core_types::account_address::AccountAddress, name: &str) -> Vec<u8> {
        use move_binary_format::file_format::*;
        let mut module = empty_module();
        module.address_identifiers[0] = addr;
        module.identifiers[0] = move_core_types::identifier::Identifier::new(name).unwrap();
        let mut buf = Vec::new();
        module.serialize_with_version(move_binary_format::file_format_common::VERSION_MAX, &mut buf)
            .expect("serialize empty module");
        buf
    }

    #[test]
    fn test_execute_contract_publish_basic() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(shared));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);

        let addr = move_core_types::account_address::AccountAddress::from_hex_literal("0xdead")
            .expect("valid addr");
        let module_bytes = make_module_bytes(addr, "counter");

        let result = executor.execute_contract_publish("alice", &[module_bytes], test_vlc());
        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.event_type, setu_types::event::EventType::ContractPublish);
        let er = event.execution_result.as_ref().unwrap();
        assert!(er.success);
        assert_eq!(er.state_changes.len(), 1);
        assert!(er.state_changes[0].key.starts_with("mod:"));
    }

    #[test]
    fn test_execute_contract_publish_empty_modules() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(shared));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);

        let result = executor.execute_contract_publish("alice", &[], test_vlc());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Empty module list"));
    }

    #[test]
    fn test_execute_contract_publish_invalid_bytecode() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(shared));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);

        let result = executor.execute_contract_publish("alice", &[vec![0xFF, 0x00]], test_vlc());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deserialization failed"));
    }

    #[test]
    fn test_execute_contract_publish_adr4_duplicate() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(Arc::clone(&shared)));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);

        let addr = move_core_types::account_address::AccountAddress::from_hex_literal("0xdead")
            .expect("valid addr");
        let module_bytes = make_module_bytes(addr, "counter");

        // First publish succeeds
        let result1 = executor.execute_contract_publish("alice", &[module_bytes.clone()], test_vlc());
        assert!(result1.is_ok());

        // Second publish of same module fails (ADR-4)
        let result2 = executor.execute_contract_publish("alice", &[module_bytes], test_vlc());
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("ADR-4"));
    }

    #[test]
    fn test_execute_contract_publish_batch_duplicate() {
        let shared = Arc::new(SharedStateManager::new(GlobalStateManager::new()));
        let provider = Arc::new(MerkleStateProvider::new(shared));
        let executor = InfraExecutor::new("validator-1".to_string(), provider);

        let addr = move_core_types::account_address::AccountAddress::from_hex_literal("0xdead")
            .expect("valid addr");
        let module_bytes = make_module_bytes(addr, "counter");

        // Same module twice in one batch
        let result = executor.execute_contract_publish(
            "alice",
            &[module_bytes.clone(), module_bytes],
            test_vlc(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate module in batch"));
    }
}
