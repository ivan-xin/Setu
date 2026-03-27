//! SetuMoveEngine — Move VM lifecycle, session creation, and execution.
//!
//! Application-level singleton created once at startup.
//! Thread-safe: MoveVM uses Arc internally.

use std::collections::HashMap;

use move_core_types::{
    effects::{ChangeSet, Op},
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
};
use move_vm_runtime::{
    move_vm::MoveVM,
    native_extensions::NativeContextExtensions,
};

use setu_runtime::{error::RuntimeError, state::RawStore};
use setu_types::object::{Address, ObjectId, Ownership};
use setu_types::envelope::ObjectEnvelope;

use crate::address_compat::move_addr_to_setu;
use crate::gas::InstructionCountGasMeter;
use crate::natives::setu_native_functions;
use crate::object_runtime::{
    InputObject, ObjectRuntimeResults, SetuObjectRuntime,
};
use crate::resolver::SetuModuleResolver;

// ═══════════════════════════════════════════════════════════════
// Config & Output types
// ═══════════════════════════════════════════════════════════════

/// Gas configuration (Phase 1: single field).
pub struct GasConfig {
    pub default_budget: u64,
}

impl Default for GasConfig {
    fn default() -> Self {
        Self {
            default_budget: 10_000_000,
        }
    }
}

/// Execution context passed from the outer runtime.
pub struct MoveExecutionContext {
    pub tx_hash: [u8; 32],
    pub sender: Address,
    pub gas_budget: u64,
    pub current_version: u64,
}

/// Module publish/change record.
#[derive(Debug, Clone)]
pub enum ModuleChange {
    Publish(ModuleId, Vec<u8>),
}

/// State change produced by Move execution.
#[derive(Debug, Clone)]
pub struct MoveStateChange {
    pub object_id: ObjectId,
    pub change_type: MoveStateChangeType,
    pub old_state: Option<Vec<u8>>,
    pub new_state: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveStateChangeType {
    Create,
    Update,
    Delete,
    Freeze,
}

/// Full output of a Move function execution.
pub struct MoveExecutionOutput {
    pub success: bool,
    pub state_changes: Vec<MoveStateChange>,
    pub module_changes: Vec<ModuleChange>,
    pub return_values: Vec<Vec<u8>>,
    pub gas_used: u64,
    pub error: Option<String>,
}

// ═══════════════════════════════════════════════════════════════
// SetuMoveEngine
// ═══════════════════════════════════════════════════════════════

/// Move VM execution engine.
///
/// Thread-safe singleton — created once, shared across tasks.
pub struct SetuMoveEngine {
    vm: MoveVM,
    /// Pre-compiled stdlib modules (ModuleId → bytecode).
    /// Empty in Phase 1; populated when stdlib is compiled.
    stdlib_modules: HashMap<ModuleId, Vec<u8>>,
    gas_config: GasConfig,
}

impl SetuMoveEngine {
    /// Create engine with no stdlib (Phase 1).
    pub fn new() -> Result<Self, RuntimeError> {
        let natives = setu_native_functions();
        let vm = MoveVM::new(natives)
            .map_err(|e| RuntimeError::VMInitError(e.to_string()))?;

        Ok(Self {
            vm,
            stdlib_modules: HashMap::new(),
            gas_config: GasConfig::default(),
        })
    }

    /// Create engine with pre-loaded stdlib modules.
    pub fn new_with_stdlib(
        stdlib_modules: HashMap<ModuleId, Vec<u8>>,
    ) -> Result<Self, RuntimeError> {
        let natives = setu_native_functions();
        let vm = MoveVM::new(natives)
            .map_err(|e| RuntimeError::VMInitError(e.to_string()))?;

        Ok(Self {
            vm,
            stdlib_modules,
            gas_config: GasConfig::default(),
        })
    }

    /// Execute a Move function call.
    #[allow(clippy::too_many_arguments)]
    pub fn execute<S: RawStore>(
        &self,
        store: &S,
        input_objects: Vec<InputObject>,
        module_id: &ModuleId,
        function: &IdentStr,
        type_args: Vec<TypeTag>,
        args: Vec<Vec<u8>>,
        ctx: &MoveExecutionContext,
    ) -> Result<MoveExecutionOutput, RuntimeError> {
        // 1. Resolver
        let resolver = SetuModuleResolver::new(store, &self.stdlib_modules);

        // 2. Object runtime
        let object_runtime =
            SetuObjectRuntime::new(input_objects, ctx.tx_hash, ctx.sender);

        // 3. Extensions
        let mut extensions = NativeContextExtensions::default();
        extensions.add(object_runtime);

        // 4. Session (consumes resolver ownership)
        let mut session = self.vm.new_session_with_extensions(resolver, extensions);

        // 5. Load type args
        let ty_args: Vec<move_vm_types::loaded_data::runtime_types::Type> = type_args
            .iter()
            .map(|tag| session.load_type(tag))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| RuntimeError::VMExecutionError(e.to_string()))?;

        // 6. Gas meter
        let mut gas_meter = InstructionCountGasMeter::new(ctx.gas_budget);

        // 7. Execute (6 args: module, func, ty_args, args, gas_meter, tracer)
        let exec_result = session.execute_function_bypass_visibility(
            module_id, function, ty_args, args, &mut gas_meter, None,
        );

        match exec_result {
            Ok(return_values) => {
                // 8. Finish session — v3.8 R8-1: two-step destructure
                let (finish_result, _resolver_back) = session.finish_with_extensions();
                let (_change_set, mut native_ext) = finish_result
                    .map_err(|e| RuntimeError::VMExecutionError(e.to_string()))?;

                // 9. Extract object runtime
                let obj_runtime: SetuObjectRuntime = native_ext.remove::<SetuObjectRuntime>()
                    .map_err(|e| RuntimeError::VMExecutionError(e.to_string()))?;
                let obj_results = obj_runtime.into_results();

                // 10. Extract module changes from ChangeSet
                let module_changes = self.extract_module_changes(&_change_set)?;

                // 11. Convert object results → state changes
                let state_changes = self.convert_results_to_state_changes(
                    &obj_results,
                    ctx.current_version,
                )?;

                // 12. Serialize return values
                let ret = return_values
                    .return_values
                    .iter()
                    .map(|(bytes, _layout)| bytes.clone())
                    .collect();

                Ok(MoveExecutionOutput {
                    success: true,
                    state_changes,
                    module_changes,
                    return_values: ret,
                    gas_used: gas_meter.instructions_executed(),
                    error: None,
                })
            }
            Err(vm_error) => Ok(MoveExecutionOutput {
                success: false,
                state_changes: vec![],
                module_changes: vec![],
                return_values: vec![],
                gas_used: gas_meter.instructions_executed(),
                error: Some(vm_error.to_string()),
            }),
        }
    }

    /// Extract module publish operations from ChangeSet.
    ///
    /// ADR-4: Module update/delete is strictly forbidden.
    fn extract_module_changes(
        &self,
        change_set: &ChangeSet,
    ) -> Result<Vec<ModuleChange>, RuntimeError> {
        let mut changes = vec![];
        // ChangeSet.modules() returns Iterator<(AccountAddress, &Identifier, Op<&[u8]>)>
        for (addr, name, op) in change_set.modules() {
            let mid = ModuleId::new(addr, name.clone());
            match op {
                Op::New(bytes) => {
                    changes.push(ModuleChange::Publish(mid, bytes.to_vec()));
                }
                Op::Modify(_) => {
                    return Err(RuntimeError::VMExecutionError(format!(
                        "Module update disabled (ADR-4): {}::{}",
                        addr.to_hex_literal(),
                        name
                    )));
                }
                Op::Delete => {
                    return Err(RuntimeError::VMExecutionError(format!(
                        "Module deletion disabled (ADR-4): {}::{}",
                        addr.to_hex_literal(),
                        name
                    )));
                }
            }
        }
        Ok(changes)
    }

    /// Convert ObjectRuntimeResults to MoveStateChange list.
    fn convert_results_to_state_changes(
        &self,
        results: &ObjectRuntimeResults,
        base_version: u64,
    ) -> Result<Vec<MoveStateChange>, RuntimeError> {
        let mut changes = vec![];
        let new_version = base_version + 1;

        // Transfers (creates + updates)
        for (id, effect) in &results.transfers {
            let new_owner = move_addr_to_setu(&effect.new_owner);
            let envelope = ObjectEnvelope::from_move_result(
                *id,
                new_owner,
                new_version,
                Ownership::AddressOwner(new_owner),
                effect.type_tag.to_string(),
                effect.bcs_bytes.clone(),
            );

            let change_type = if results.created_ids.contains(id) {
                MoveStateChangeType::Create
            } else {
                MoveStateChangeType::Update
            };

            changes.push(MoveStateChange {
                object_id: *id,
                change_type,
                old_state: results
                    .input_objects
                    .get(id)
                    .map(|inp| inp.envelope_bytes.clone()),
                new_state: Some(envelope.to_bytes()),
            });
        }

        // Mutations (&mut objects modified in-place)
        for (id, effect) in &results.mutated {
            if results.transfers.contains_key(id) {
                continue; // transfer takes precedence
            }
            let input = results.input_objects.get(id);
            let owner = input
                .map(|i| i.owner)
                .unwrap_or(Address::ZERO);
            let ownership = input
                .map(|i| Ownership::AddressOwner(i.owner))
                .unwrap_or(Ownership::AddressOwner(Address::ZERO));

            let envelope = ObjectEnvelope::from_move_result(
                *id,
                owner,
                new_version,
                ownership,
                effect.type_tag.to_string(),
                effect.bcs_bytes.clone(),
            );

            changes.push(MoveStateChange {
                object_id: *id,
                change_type: MoveStateChangeType::Update,
                old_state: input.map(|i| i.envelope_bytes.clone()),
                new_state: Some(envelope.to_bytes()),
            });
        }

        // Frozen objects
        for (id, effect) in &results.frozen {
            let input = results.input_objects.get(id);
            let owner = input
                .map(|i| i.owner)
                .unwrap_or(Address::ZERO);

            let envelope = ObjectEnvelope::from_move_result(
                *id,
                owner,
                new_version,
                Ownership::Immutable,
                effect.type_tag.to_string(),
                effect.bcs_bytes.clone(),
            );

            changes.push(MoveStateChange {
                object_id: *id,
                change_type: MoveStateChangeType::Freeze,
                old_state: input.map(|i| i.envelope_bytes.clone()),
                new_state: Some(envelope.to_bytes()),
            });
        }

        // Deletions
        for id in &results.deleted_ids {
            changes.push(MoveStateChange {
                object_id: *id,
                change_type: MoveStateChangeType::Delete,
                old_state: results
                    .input_objects
                    .get(id)
                    .map(|i| i.envelope_bytes.clone()),
                new_state: None,
            });
        }

        Ok(changes)
    }

    pub fn gas_config(&self) -> &GasConfig {
        &self.gas_config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = SetuMoveEngine::new();
        assert!(engine.is_ok(), "Engine should initialize with empty natives");
    }

    #[test]
    fn test_engine_with_empty_stdlib() {
        let engine = SetuMoveEngine::new_with_stdlib(HashMap::new());
        assert!(engine.is_ok());
    }

    #[test]
    fn test_gas_config_default() {
        let engine = SetuMoveEngine::new().unwrap();
        assert_eq!(engine.gas_config().default_budget, 10_000_000);
    }

    #[test]
    fn test_state_change_type_eq() {
        assert_eq!(MoveStateChangeType::Create, MoveStateChangeType::Create);
        assert_ne!(MoveStateChangeType::Create, MoveStateChangeType::Delete);
    }
}
