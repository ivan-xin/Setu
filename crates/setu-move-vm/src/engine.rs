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

// Embed build-time generated stdlib bytecodes constant.
// When .mv files exist in setu-framework/compiled/, STDLIB_MODULES is populated.
// Otherwise it's an empty array (build.rs graceful degradation).
include!(concat!(env!("OUT_DIR"), "/stdlib_modules.rs"));

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
    /// Epoch number (Phase 3: fixed 0).
    pub epoch: u64,
    /// Whether the target function takes `&mut TxContext` as last parameter.
    pub needs_tx_context: bool,
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
    pub events: Vec<(String, Vec<u8>)>,
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

    /// Create engine with embedded stdlib modules (from build.rs).
    ///
    /// Parses each (name, bytecode) pair from [`STDLIB_MODULES`],
    /// deserializes the bytecode to extract the real `ModuleId`,
    /// verifies it with move-bytecode-verifier, then passes the
    /// resulting HashMap to [`new_with_stdlib`].
    ///
    /// Returns an engine with an empty stdlib if no .mv files were
    /// found at build time (graceful degradation).
    pub fn new_with_embedded_stdlib() -> Result<Self, RuntimeError> {
        let mut stdlib = HashMap::new();

        for &(name, bytes) in STDLIB_MODULES {
            let compiled_module =
                move_binary_format::CompiledModule::deserialize_with_defaults(bytes)
                    .map_err(|e| {
                        RuntimeError::VMInitError(format!(
                            "Failed to deserialize stdlib module '{}': {}",
                            name, e
                        ))
                    })?;

            move_bytecode_verifier::verify_module_unmetered(&compiled_module).map_err(
                |e| {
                    RuntimeError::VMInitError(format!(
                        "Stdlib module '{}' failed verification: {}",
                        name, e
                    ))
                },
            )?;

            let module_id = compiled_module.self_id();
            stdlib.insert(module_id, bytes.to_vec());
        }

        if stdlib.is_empty() {
            tracing::warn!(
                "No stdlib modules found. \
                 Run: cd setu-framework && sui move build && \
                 cp build/SetuFramework/bytecode_modules/*.mv compiled/"
            );
        } else {
            tracing::info!("Loaded {} stdlib modules", stdlib.len());
        }

        Self::new_with_stdlib(stdlib)
    }

    /// Returns the number of embedded stdlib modules.
    pub fn stdlib_module_count(&self) -> usize {
        self.stdlib_modules.len()
    }

    /// Execute a Move function call.
    ///
    /// `mutable_arg_map` maps each `&mut` argument's position in `args`
    /// to its input ObjectId, so that post-execution mutations captured
    /// in `mutable_reference_outputs` can be recorded as state changes.
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
        mutable_arg_map: &[(usize, ObjectId)],
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

        // 6.5. Append TxContext BCS if the target function requires it
        let mut final_args = args;
        if ctx.needs_tx_context {
            final_args.push(Self::build_tx_context_bcs(ctx));
        }

        // 7. Execute (6 args: module, func, ty_args, args, gas_meter, tracer)
        let exec_result = session.execute_function_bypass_visibility(
            module_id, function, ty_args, final_args, &mut gas_meter, None,
        );

        match exec_result {
            Ok(return_values) => {
                // 8. Finish session — v3.8 R8-1: two-step destructure
                let (finish_result, _resolver_back) = session.finish_with_extensions();
                let (_change_set, mut native_ext) = finish_result
                    .map_err(|e| RuntimeError::VMExecutionError(e.to_string()))?;

                // 9. Extract object runtime
                let mut obj_runtime: SetuObjectRuntime = native_ext.remove::<SetuObjectRuntime>()
                    .map_err(|e| RuntimeError::VMExecutionError(e.to_string()))?;

                // 9.5. Process mutable_reference_outputs — capture &mut arg mutations
                // The Move VM serializes mutated &mut arguments back in
                // SerializedReturnValues.mutable_reference_outputs.
                // Each entry: (LocalIndex = arg position, bcs_bytes, layout).
                for (local_idx, bcs_bytes, _layout) in &return_values.mutable_reference_outputs {
                    let arg_idx = *local_idx as usize;
                    if let Some(&(_, object_id)) = mutable_arg_map.iter()
                        .find(|(pos, _)| *pos == arg_idx)
                    {
                        // Look up the original input object to get its StructTag
                        if let Some(input_obj) = obj_runtime.get_input_object(&object_id) {
                            let type_tag = input_obj.type_tag.clone();
                            obj_runtime.mutate_object(object_id, type_tag, bcs_bytes.clone());
                        } else {
                            tracing::warn!(
                                arg_idx,
                                object_id = %object_id,
                                "Input object not found for mutated &mut arg, skipping"
                            );
                        }
                    }
                }

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

                let events = obj_results.emitted_events
                    .iter()
                    .map(|(tag, bytes)| (tag.to_string(), bytes.clone()))
                    .collect();

                Ok(MoveExecutionOutput {
                    success: true,
                    state_changes,
                    module_changes,
                    return_values: ret,
                    events,
                    gas_used: gas_meter.instructions_executed(),
                    error: None,
                })
            }
            Err(vm_error) => Ok(MoveExecutionOutput {
                success: false,
                state_changes: vec![],
                module_changes: vec![],
                return_values: vec![],
                events: vec![],
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

    /// Build BCS-serialized TxContext for Move function calls.
    ///
    /// Layout matches `0x2::tx_context::TxContext` (Sui stdlib):
    /// ```text
    /// struct TxContext has drop {
    ///     sender: address,
    ///     tx_hash: vector<u8>,
    ///     epoch: u64,
    ///     ids_created: u64,
    /// }
    /// ```
    pub fn build_tx_context_bcs(ctx: &MoveExecutionContext) -> Vec<u8> {
        use crate::address_compat::setu_addr_to_move;
        let mut buf = Vec::new();
        // sender: address (32 bytes, fixed-size in BCS)
        let move_addr = setu_addr_to_move(&ctx.sender);
        buf.extend_from_slice(move_addr.as_ref());
        // tx_hash: vector<u8> (BCS: ULEB128 length + bytes)
        let tx_hash_bcs = bcs::to_bytes(&ctx.tx_hash.to_vec())
            .expect("BCS serialize tx_hash");
        buf.extend_from_slice(&tx_hash_bcs);
        // epoch: u64
        buf.extend_from_slice(&ctx.epoch.to_le_bytes());
        // ids_created: u64 (starts at 0)
        buf.extend_from_slice(&0u64.to_le_bytes());
        buf
    }

    pub fn gas_config(&self) -> &GasConfig {
        &self.gas_config
    }
}

// ═══════════════════════════════════════════════════════════════
// needs_tx_context auto-detection
// ═══════════════════════════════════════════════════════════════

/// Detect whether a Move function's last parameter is `&mut TxContext`.
///
/// Deserializes the module bytecode, finds the named function, and inspects
/// the last parameter's type signature. Returns `true` if the last param
/// is `MutableReference(Struct(0x1::tx_context::TxContext))`.
///
/// Returns `None` if the module cannot be parsed or the function is not found.
pub fn detect_needs_tx_context(module_bytes: &[u8], function_name: &str) -> Option<bool> {
    use move_binary_format::CompiledModule;
    use move_binary_format::file_format::SignatureToken;

    let module = CompiledModule::deserialize_with_defaults(module_bytes).ok()?;

    // Find the function definition
    for func_def in &module.function_defs {
        let func_handle = &module.function_handles[func_def.function.0 as usize];
        let func_ident = module.identifier_at(func_handle.name);
        if func_ident.as_str() != function_name {
            continue;
        }

        let sig = &module.signatures[func_handle.parameters.0 as usize];
        let params = &sig.0;
        if let Some(last_param) = params.last() {
            // Check: MutableReference(Datatype(idx)) where datatype is 0x1::tx_context::TxContext
            if let SignatureToken::MutableReference(inner) = last_param {
                if let SignatureToken::Datatype(datatype_handle_idx) = inner.as_ref() {
                    let dt_handle = &module.datatype_handles[datatype_handle_idx.0 as usize];
                    let struct_name = module.identifier_at(dt_handle.name);
                    let module_handle = &module.module_handles[dt_handle.module.0 as usize];
                    let module_name = module.identifier_at(module_handle.name);
                    let module_addr = module.address_identifier_at(module_handle.address);

                    // Check: address == 0x1, module == "tx_context", struct == "TxContext"
                    if module_addr == &move_core_types::account_address::AccountAddress::ONE
                        && module_name.as_str() == "tx_context"
                        && struct_name.as_str() == "TxContext"
                    {
                        return Some(true);
                    }
                }
            }
            return Some(false);
        }
        // No parameters → no TxContext
        return Some(false);
    }
    None // function not found
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

    // ═══════════════════════════════════════════════════════════
    // Phase 2: Stdlib embedding tests
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_embedded_stdlib_empty_ok() {
        // new_with_embedded_stdlib should succeed even when STDLIB_MODULES is empty
        let engine = SetuMoveEngine::new_with_embedded_stdlib();
        assert!(engine.is_ok(), "Engine should init with empty or populated stdlib");
    }

    #[test]
    fn test_embedded_stdlib_loads_all() {
        if STDLIB_MODULES.is_empty() {
            eprintln!("SKIP: stdlib .mv files not found — run sui move build");
            return;
        }
        let engine = SetuMoveEngine::new_with_embedded_stdlib().unwrap();
        assert_eq!(
            engine.stdlib_module_count(),
            12,
            "Expected 12 stdlib modules (object, transfer, tx_context, balance, coin, setu, vector, option, string, vec_map, vec_set, event)"
        );
    }

    #[test]
    fn test_embedded_stdlib_module_ids_at_0x1() {
        if STDLIB_MODULES.is_empty() {
            eprintln!("SKIP: stdlib .mv files not found");
            return;
        }
        use move_core_types::account_address::AccountAddress;

        for &(name, bytes) in STDLIB_MODULES {
            let cm =
                move_binary_format::CompiledModule::deserialize_with_defaults(bytes)
                    .unwrap_or_else(|e| panic!("Failed to deserialize {}: {}", name, e));
            assert_eq!(
                *cm.self_id().address(),
                AccountAddress::ONE,
                "Module '{}' should be at address 0x1",
                name
            );
        }
    }

    #[test]
    fn test_embedded_stdlib_module_names() {
        if STDLIB_MODULES.is_empty() {
            eprintln!("SKIP: stdlib .mv files not found");
            return;
        }
        let expected = ["object", "transfer", "tx_context", "balance", "coin", "setu",
                       "vector", "option", "string", "vec_map", "vec_set"];
        let names: Vec<&str> = STDLIB_MODULES.iter().map(|(n, _)| *n).collect();
        for exp in &expected {
            assert!(
                names.contains(exp),
                "Expected module '{}' in STDLIB_MODULES, found: {:?}",
                exp,
                names
            );
        }
    }

    #[test]
    fn test_embedded_stdlib_resolver_finds_modules() {
        if STDLIB_MODULES.is_empty() {
            eprintln!("SKIP: stdlib .mv files not found");
            return;
        }
        use setu_runtime::state::InMemoryObjectStore;
        use move_core_types::resolver::ModuleResolver;
        use crate::resolver::SetuModuleResolver;

        let engine = SetuMoveEngine::new_with_embedded_stdlib().unwrap();
        let store = InMemoryObjectStore::new();
        let resolver = SetuModuleResolver::new(&store, &engine.stdlib_modules);

        for &(name, _) in STDLIB_MODULES {
            let cm =
                move_binary_format::CompiledModule::deserialize_with_defaults(
                    STDLIB_MODULES
                        .iter()
                        .find(|(n, _)| *n == name)
                        .unwrap()
                        .1,
                )
                .unwrap();
            let mid = cm.self_id();
            let result = resolver.get_module(&mid);
            assert!(
                result.is_ok() && result.unwrap().is_some(),
                "Resolver should find stdlib module '{}'",
                name
            );
        }
    }

    #[test]
    fn test_embedded_stdlib_bytecode_verifies() {
        if STDLIB_MODULES.is_empty() {
            eprintln!("SKIP: stdlib .mv files not found");
            return;
        }
        for &(name, bytes) in STDLIB_MODULES {
            let cm =
                move_binary_format::CompiledModule::deserialize_with_defaults(bytes)
                    .unwrap_or_else(|e| panic!("Failed to deserialize {}: {}", name, e));
            move_bytecode_verifier::verify_module_unmetered(&cm)
                .unwrap_or_else(|e| panic!("Module '{}' failed verification: {}", name, e));
        }
    }

    // ── detect_needs_tx_context tests ──

    fn stdlib_bytes(name: &str) -> &'static [u8] {
        STDLIB_MODULES
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("stdlib module '{}' not found", name))
            .1
    }

    #[test]
    fn test_detect_needs_tx_context_true() {
        // coin::from_balance(balance, ctx: &mut TxContext) → should detect true
        if STDLIB_MODULES.is_empty() {
            eprintln!("SKIP: no stdlib");
            return;
        }
        let result = detect_needs_tx_context(stdlib_bytes("coin"), "from_balance");
        assert_eq!(result, Some(true), "from_balance has &mut TxContext as last param");
    }

    #[test]
    fn test_detect_needs_tx_context_false_immutable_ref() {
        // tx_context::sender(ctx: &TxContext) — immutable ref, not mutable → false
        if STDLIB_MODULES.is_empty() {
            eprintln!("SKIP: no stdlib");
            return;
        }
        let result = detect_needs_tx_context(stdlib_bytes("tx_context"), "sender");
        assert_eq!(result, Some(false), "sender takes &TxContext (immutable), not &mut");
    }

    #[test]
    fn test_detect_needs_tx_context_false_no_context() {
        // transfer::transfer<T>(obj, recipient: address) — no TxContext param at all
        if STDLIB_MODULES.is_empty() {
            eprintln!("SKIP: no stdlib");
            return;
        }
        let result = detect_needs_tx_context(stdlib_bytes("transfer"), "transfer");
        assert_eq!(result, Some(false), "transfer has no TxContext param");
    }

    #[test]
    fn test_detect_needs_tx_context_function_not_found() {
        if STDLIB_MODULES.is_empty() {
            eprintln!("SKIP: no stdlib");
            return;
        }
        let result = detect_needs_tx_context(stdlib_bytes("coin"), "nonexistent_function");
        assert_eq!(result, None, "non-existent function should return None");
    }

    #[test]
    fn test_detect_needs_tx_context_invalid_bytecode() {
        let result = detect_needs_tx_context(b"not valid bytecode", "anything");
        assert_eq!(result, None, "invalid bytecode should return None (deserialization fails)");
    }
}
