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
use setu_types::dynamic_field::DfFieldValue;
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
    /// Epoch timestamp in milliseconds (injected by consensus, not host clock).
    pub epoch_timestamp_ms: u64,
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
    Share,
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
// Dynamic Field envelope builder (M3)
// ═══════════════════════════════════════════════════════════════

/// Build the `ObjectEnvelope` for a dynamic-field entry.
///
/// Contract (see DF FDP v1.5 §3.7):
/// - `owner = Address::ZERO` — DF entries have no direct owner
/// - `ownership = Ownership::ObjectOwner(parent)` — makes apply-layer
///   byte-level conflict detection work without any change to storage
/// - `type_tag = "0x1::dynamic_field::Field<{K}, {V}>"` — canonical form
/// - `data = bcs(DfFieldValue { name_type_tag, name_bcs, value_type_tag, value_bcs })`
fn build_df_envelope(
    df_oid: ObjectId,
    parent: ObjectId,
    key_type_tag: &str,
    key_bcs: &[u8],
    value_type_tag: &str,
    value_bcs: &[u8],
    version: u64,
) -> ObjectEnvelope {
    let payload = DfFieldValue {
        name_type_tag: key_type_tag.to_string(),
        name_bcs: key_bcs.to_vec(),
        value_type_tag: value_type_tag.to_string(),
        value_bcs: value_bcs.to_vec(),
    };
    // BCS of DfFieldValue is deterministic — used by R1 layer for
    // byte-level conflict detection.
    let data_bcs = bcs::to_bytes(&payload)
        .expect("DfFieldValue BCS serialization is infallible for owned data");
    ObjectEnvelope::from_move_result(
        df_oid,
        Address::ZERO,
        version,
        Ownership::ObjectOwner(parent),
        format!(
            "0x1::dynamic_field::Field<{}, {}>",
            key_type_tag, value_type_tag
        ),
        data_bcs,
    )
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
        df_preload: Vec<setu_types::task::ResolvedDynamicField>,
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
        let object_runtime = SetuObjectRuntime::new(
            input_objects,
            df_preload,
            ctx.tx_hash,
            ctx.sender,
            ctx.epoch_timestamp_ms,
        );

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

        // Transfers (creates + updates). Skip objects that were shared or
        // frozen in the same TX — share/freeze are terminal ownership transitions.
        for (id, effect) in &results.transfers {
            if results.shared.contains_key(id) || results.frozen.contains_key(id) {
                continue;
            }
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
            if results.transfers.contains_key(id)
                || results.shared.contains_key(id)
                || results.frozen.contains_key(id)
            {
                continue; // transfer / share / freeze take precedence
            }
            let input = results.input_objects.get(id);

            // §3.7a — Parent byte-preservation rule (DF FDP v1.1 R1-ISSUE-2).
            // If the Move VM borrowed `&mut parent` but wrote no changes to
            // the parent's data bytes (typical of `df::add(&mut pool.id, ..)`),
            // skip emitting a StateChange and do NOT bump version. This keeps
            // concurrent DF mutations on shared parents PWOO-compatible.
            if let Some(inp) = input {
                if effect.bcs_bytes == inp.move_data {
                    continue;
                }
            }

            let owner = input
                .map(|i| i.owner)
                .unwrap_or(Address::ZERO);
            // R4-ISSUE-2: preserve the original ownership enum.
            // InputObject carries `ownership` so &mut mutations of Shared /
            // Immutable / ObjectOwner inputs persist back with the correct
            // Ownership variant instead of being silently demoted to
            // AddressOwner.
            let ownership = input
                .map(|i| i.ownership)
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

        // Shared objects (PWOO): Ownership → Shared { initial_shared_version }.
        // For newly-created + same-TX share the effect's initial_shared_version
        // is 0; the envelope is persisted with version bumped to `new_version`,
        // so downstream references must use expected_version=new_version.
        for (id, effect) in &results.shared {
            let input = results.input_objects.get(id);
            // Preserve the original owner in `envelope.owner` for historical
            // traceability; the `ownership` enum is the source of truth.
            let owner = input.map(|i| i.owner).unwrap_or(Address::ZERO);

            let envelope = ObjectEnvelope::from_move_result(
                *id,
                owner,
                new_version,
                Ownership::Shared {
                    initial_shared_version: effect.initial_shared_version,
                },
                effect.type_tag.to_string(),
                effect.bcs_bytes.clone(),
            );

            let change_type = if results.created_ids.contains(id) {
                MoveStateChangeType::Create
            } else {
                MoveStateChangeType::Share
            };

            changes.push(MoveStateChange {
                object_id: *id,
                change_type,
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

        // ─── M3: Dynamic Field effects → StateChange ───
        //
        // Each DF entry lives in its own SMT slot keyed by its derived
        // `df_oid`; the envelope's `ownership = ObjectOwner(parent)` so
        // byte-level conflict detection at the apply layer works unchanged.
        //
        // The `old_version` used when rebuilding the old envelope for
        // Mutate/Delete defaults to `base_version`. Full on-disk version
        // precision requires TaskPreparer to carry the on-disk envelope
        // bytes in `ResolvedDynamicField` — deferred to M4. The byte-level
        // conflict check still functions correctly because `value_bcs` is
        // the load-bearing field.

        // DF creates
        for (df_oid, eff) in &results.df_created {
            let envelope = build_df_envelope(
                *df_oid,
                eff.parent,
                &eff.key_type_tag,
                &eff.key_bcs,
                &eff.value_type_tag,
                &eff.value_bcs,
                new_version,
            );
            changes.push(MoveStateChange {
                object_id: *df_oid,
                change_type: MoveStateChangeType::Create,
                old_state: None,
                new_state: Some(envelope.to_bytes()),
            });
        }

        // DF mutations
        for (df_oid, eff) in &results.df_mutated {
            let old_env = build_df_envelope(
                *df_oid,
                eff.parent,
                &eff.key_type_tag,
                &eff.key_bcs,
                &eff.value_type_tag,
                &eff.old_value_bcs,
                base_version,
            );
            let new_env = build_df_envelope(
                *df_oid,
                eff.parent,
                &eff.key_type_tag,
                &eff.key_bcs,
                &eff.value_type_tag,
                &eff.new_value_bcs,
                new_version,
            );
            changes.push(MoveStateChange {
                object_id: *df_oid,
                change_type: MoveStateChangeType::Update,
                old_state: Some(old_env.to_bytes()),
                new_state: Some(new_env.to_bytes()),
            });
        }

        // DF deletions
        for (df_oid, eff) in &results.df_deleted {
            let old_env = build_df_envelope(
                *df_oid,
                eff.parent,
                &eff.key_type_tag,
                &eff.key_bcs,
                &eff.value_type_tag,
                &eff.old_value_bcs,
                base_version,
            );
            changes.push(MoveStateChange {
                object_id: *df_oid,
                change_type: MoveStateChangeType::Delete,
                old_state: Some(old_env.to_bytes()),
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
            15,
            "Expected 15 stdlib modules (object, transfer, tx_context, balance, coin, setu, vector, option, string, vec_map, vec_set, event, clock, access_control, dynamic_field)"
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
                       "vector", "option", "string", "vec_map", "vec_set", "event", "clock"];
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

    // R4-ISSUE-2 regression: mutating a Shared input via `&mut` must NOT
    // silently demote the persisted envelope to AddressOwner.
    #[test]
    fn test_mutated_shared_input_preserves_shared_ownership() {
        use crate::object_runtime::{
            InputObject, ObjectMutationEffect, ObjectRuntimeResults,
        };
        use indexmap::{IndexMap, IndexSet};
        use move_core_types::{
            account_address::AccountAddress, identifier::Identifier,
            language_storage::StructTag,
        };
        use setu_types::{
            object::{Address, ObjectId},
            Ownership,
        };
        use std::collections::BTreeMap;

        let engine = SetuMoveEngine::new().expect("engine init");

        let id = ObjectId::new([0x42; 32]);
        let owner = Address::new([0x11; 32]);
        let isv = 5u64;
        let tag = StructTag {
            address: AccountAddress::ONE,
            module: Identifier::new("pool").unwrap(),
            name: Identifier::new("Pool").unwrap(),
            type_params: vec![],
        };

        // Build a Shared input — this is exactly what the executor gives us
        // when a `shared_object_ids` entry is loaded.
        let pre_envelope = setu_types::ObjectEnvelope::from_move_result(
            id,
            owner,
            7, // pre-mutation version
            Ownership::Shared { initial_shared_version: isv },
            tag.to_string(),
            vec![],
        );
        let input = InputObject {
            id,
            owner,
            ownership: Ownership::Shared { initial_shared_version: isv },
            version: 7,
            envelope_bytes: pre_envelope.to_bytes(),
            move_data: vec![],
            type_tag: tag.clone(),
        };

        let mut input_objects = BTreeMap::new();
        input_objects.insert(id, input);
        let mut mutated = IndexMap::new();
        mutated.insert(
            id,
            ObjectMutationEffect { type_tag: tag, bcs_bytes: vec![0xDE, 0xAD] },
        );
        let results = ObjectRuntimeResults {
            input_objects,
            created_ids: IndexSet::new(),
            deleted_ids: IndexSet::new(),
            transfers: IndexMap::new(),
            frozen: IndexMap::new(),
            shared: IndexMap::new(),
            mutated,
            emitted_events: vec![],
            df_created: IndexMap::new(),
            df_mutated: IndexMap::new(),
            df_deleted: IndexMap::new(),
        };

        let changes = engine
            .convert_results_to_state_changes(&results, 7)
            .expect("convert ok");
        assert_eq!(changes.len(), 1, "one mutation → one state change");
        let new_bytes = changes[0]
            .new_state
            .as_ref()
            .expect("mutated emits new_state");
        let new_env = setu_types::ObjectEnvelope::from_bytes(new_bytes)
            .expect("envelope roundtrips");

        match new_env.metadata.ownership {
            Ownership::Shared { initial_shared_version } => {
                assert_eq!(
                    initial_shared_version, isv,
                    "R4-ISSUE-2: initial_shared_version preserved across &mut"
                );
            }
            other => panic!(
                "R4-ISSUE-2: expected Shared after &mut on Shared input, got {:?}",
                other
            ),
        }
        assert_eq!(new_env.metadata.version, 8, "version bumped by 1");
    }

    // ═══════════════════════════════════════════════════════════
    // M3: Dynamic Field state-change conversion tests
    // ═══════════════════════════════════════════════════════════

    mod m3_df_convert {
        use super::*;
        use crate::object_runtime::{
            DfDeleteEffect, DfMutateEffect, ObjectMutationEffect,
        };
        use indexmap::{IndexMap, IndexSet};
        use setu_types::{
            dynamic_field::DfFieldValue,
            object::{Address, ObjectId},
            Ownership,
        };
        use std::collections::BTreeMap;

        fn empty_results() -> ObjectRuntimeResults {
            ObjectRuntimeResults {
                input_objects: BTreeMap::new(),
                created_ids: IndexSet::new(),
                deleted_ids: IndexSet::new(),
                transfers: IndexMap::new(),
                frozen: IndexMap::new(),
                shared: IndexMap::new(),
                mutated: IndexMap::new(),
                emitted_events: vec![],
                df_created: IndexMap::new(),
                df_mutated: IndexMap::new(),
                df_deleted: IndexMap::new(),
            }
        }

        fn engine() -> SetuMoveEngine {
            SetuMoveEngine::new().expect("engine init")
        }

        /// E1: df_created → single Create StateChange; envelope decodes
        /// with ownership = ObjectOwner(parent); DfFieldValue roundtrips.
        #[test]
        fn test_df_create_emits_create_statechange() {
            let parent = ObjectId::new([0x10; 32]);
            let df_oid = ObjectId::new([0xD1; 32]);
            let key = vec![1u8; 8];
            let val = vec![2u8; 8];

            let mut results = empty_results();
            results.df_created.insert(
                df_oid,
                crate::object_runtime::DfCreateEffect {
                    parent,
                    key_type_tag: "u64".into(),
                    key_bcs: key.clone(),
                    value_type_tag: "u64".into(),
                    value_bcs: val.clone(),
                },
            );

            let changes = engine()
                .convert_results_to_state_changes(&results, 3)
                .expect("convert ok");
            assert_eq!(changes.len(), 1);
            let c = &changes[0];
            assert_eq!(c.change_type, MoveStateChangeType::Create);
            assert_eq!(c.object_id, df_oid);
            assert!(c.old_state.is_none());
            let env_bytes = c.new_state.as_ref().expect("new_state set");
            let env = setu_types::ObjectEnvelope::from_bytes(env_bytes).unwrap();
            match env.metadata.ownership {
                Ownership::ObjectOwner(p) => assert_eq!(p, parent),
                other => panic!("expected ObjectOwner, got {:?}", other),
            }
            assert_eq!(env.metadata.version, 4);
            let payload: DfFieldValue = bcs::from_bytes(&env.data).unwrap();
            assert_eq!(payload.name_type_tag, "u64");
            assert_eq!(payload.name_bcs, key);
            assert_eq!(payload.value_type_tag, "u64");
            assert_eq!(payload.value_bcs, val);
        }

        /// E2: df_mutated → Update StateChange with old(base_version) and
        /// new(base_version+1) envelopes.
        #[test]
        fn test_df_mutate_emits_update_statechange() {
            let parent = ObjectId::new([0x11; 32]);
            let df_oid = ObjectId::new([0xD2; 32]);

            let mut results = empty_results();
            results.df_mutated.insert(
                df_oid,
                DfMutateEffect {
                    parent,
                    key_type_tag: "u64".into(),
                    key_bcs: vec![7; 8],
                    value_type_tag: "u64".into(),
                    old_value_bcs: vec![0xA; 8],
                    new_value_bcs: vec![0xB; 8],
                },
            );

            let base = 9u64;
            let changes = engine()
                .convert_results_to_state_changes(&results, base)
                .expect("convert ok");
            assert_eq!(changes.len(), 1);
            let c = &changes[0];
            assert_eq!(c.change_type, MoveStateChangeType::Update);
            assert_eq!(c.object_id, df_oid);
            let old = setu_types::ObjectEnvelope::from_bytes(
                c.old_state.as_ref().unwrap(),
            )
            .unwrap();
            let new = setu_types::ObjectEnvelope::from_bytes(
                c.new_state.as_ref().unwrap(),
            )
            .unwrap();
            assert_eq!(old.metadata.version, base);
            assert_eq!(new.metadata.version, base + 1);
            let old_pl: DfFieldValue = bcs::from_bytes(&old.data).unwrap();
            let new_pl: DfFieldValue = bcs::from_bytes(&new.data).unwrap();
            assert_eq!(old_pl.value_bcs, vec![0xA; 8]);
            assert_eq!(new_pl.value_bcs, vec![0xB; 8]);
        }

        /// E3: df_deleted → Delete StateChange, only old_state set at
        /// base_version; new_state is None.
        #[test]
        fn test_df_delete_emits_delete_statechange() {
            let parent = ObjectId::new([0x12; 32]);
            let df_oid = ObjectId::new([0xD3; 32]);

            let mut results = empty_results();
            results.df_deleted.insert(
                df_oid,
                DfDeleteEffect {
                    parent,
                    key_type_tag: "address".into(),
                    key_bcs: vec![0xCD; 32],
                    value_type_tag: "u64".into(),
                    old_value_bcs: vec![0x99; 8],
                },
            );

            let base = 11u64;
            let changes = engine()
                .convert_results_to_state_changes(&results, base)
                .expect("convert ok");
            assert_eq!(changes.len(), 1);
            let c = &changes[0];
            assert_eq!(c.change_type, MoveStateChangeType::Delete);
            assert_eq!(c.object_id, df_oid);
            assert!(c.new_state.is_none());
            let old = setu_types::ObjectEnvelope::from_bytes(
                c.old_state.as_ref().unwrap(),
            )
            .unwrap();
            assert_eq!(old.metadata.version, base);
            let pl: DfFieldValue = bcs::from_bytes(&old.data).unwrap();
            assert_eq!(pl.value_bcs, vec![0x99; 8]);
        }

        /// E4: DF ordering — create / mutate / delete conversions run
        /// after parent-level (mutated/transfers/…). No panics, lengths add.
        #[test]
        fn test_df_changes_appended_after_parent_changes() {
            let parent = ObjectId::new([0x13; 32]);
            let df_a = ObjectId::new([0xDA; 32]);
            let df_b = ObjectId::new([0xDB; 32]);
            let df_c = ObjectId::new([0xDC; 32]);

            let mut results = empty_results();
            results.df_created.insert(
                df_a,
                crate::object_runtime::DfCreateEffect {
                    parent,
                    key_type_tag: "u64".into(),
                    key_bcs: vec![1; 8],
                    value_type_tag: "u64".into(),
                    value_bcs: vec![1; 8],
                },
            );
            results.df_mutated.insert(
                df_b,
                DfMutateEffect {
                    parent,
                    key_type_tag: "u64".into(),
                    key_bcs: vec![2; 8],
                    value_type_tag: "u64".into(),
                    old_value_bcs: vec![2; 8],
                    new_value_bcs: vec![3; 8],
                },
            );
            results.df_deleted.insert(
                df_c,
                DfDeleteEffect {
                    parent,
                    key_type_tag: "u64".into(),
                    key_bcs: vec![3; 8],
                    value_type_tag: "u64".into(),
                    old_value_bcs: vec![4; 8],
                },
            );

            let changes = engine()
                .convert_results_to_state_changes(&results, 1)
                .expect("convert ok");
            assert_eq!(changes.len(), 3);
            assert_eq!(changes[0].change_type, MoveStateChangeType::Create);
            assert_eq!(changes[1].change_type, MoveStateChangeType::Update);
            assert_eq!(changes[2].change_type, MoveStateChangeType::Delete);
        }

        /// E5: §3.7a parent byte-preservation — if mutated[oid].bcs_bytes
        /// equals the input's move_data, no StateChange is emitted.
        #[test]
        fn test_parent_bytes_preserved_skips_statechange() {
            use move_core_types::{
                account_address::AccountAddress, identifier::Identifier,
                language_storage::StructTag,
            };

            let parent = ObjectId::new([0x14; 32]);
            let owner = Address::new([0x55; 32]);
            let tag = StructTag {
                address: AccountAddress::ONE,
                module: Identifier::new("m").unwrap(),
                name: Identifier::new("T").unwrap(),
                type_params: vec![],
            };
            let data = vec![0xAB, 0xCD];

            let pre = setu_types::ObjectEnvelope::from_move_result(
                parent,
                owner,
                4,
                Ownership::AddressOwner(owner),
                tag.to_string(),
                data.clone(),
            );
            let input = InputObject {
                id: parent,
                owner,
                ownership: Ownership::AddressOwner(owner),
                version: 4,
                envelope_bytes: pre.to_bytes(),
                move_data: data.clone(),
                type_tag: tag.clone(),
            };
            let mut results = empty_results();
            results.input_objects.insert(parent, input);
            results.mutated.insert(
                parent,
                ObjectMutationEffect { type_tag: tag, bcs_bytes: data },
            );

            let changes = engine()
                .convert_results_to_state_changes(&results, 4)
                .expect("convert ok");
            assert!(
                changes.is_empty(),
                "byte-identical mutation must not emit StateChange"
            );
        }

        /// E6: Versioning — df Create / Update / Delete all use base+1 for
        /// new envelopes and base for old.
        #[test]
        fn test_df_versioning_uses_base_plus_one() {
            let parent = ObjectId::new([0x15; 32]);
            let df_oid = ObjectId::new([0xD5; 32]);

            let mut results = empty_results();
            results.df_created.insert(
                df_oid,
                crate::object_runtime::DfCreateEffect {
                    parent,
                    key_type_tag: "u64".into(),
                    key_bcs: vec![0; 8],
                    value_type_tag: "u64".into(),
                    value_bcs: vec![0; 8],
                },
            );

            let base = 42u64;
            let changes = engine()
                .convert_results_to_state_changes(&results, base)
                .expect("convert ok");
            let env = setu_types::ObjectEnvelope::from_bytes(
                changes[0].new_state.as_ref().unwrap(),
            )
            .unwrap();
            assert_eq!(env.metadata.version, base + 1);
        }

        /// E7: DfFieldValue BCS payload shape — verify key/value are
        /// stored verbatim, not re-serialized.
        #[test]
        fn test_df_field_value_bcs_shape() {
            let parent = ObjectId::new([0x16; 32]);
            let df_oid = ObjectId::new([0xD6; 32]);
            let raw_key = vec![0xDE, 0xAD, 0xBE, 0xEF];
            let raw_val = vec![0xCA, 0xFE];

            let mut results = empty_results();
            results.df_created.insert(
                df_oid,
                crate::object_runtime::DfCreateEffect {
                    parent,
                    key_type_tag: "vector<u8>".into(),
                    key_bcs: raw_key.clone(),
                    value_type_tag: "vector<u8>".into(),
                    value_bcs: raw_val.clone(),
                },
            );

            let changes = engine()
                .convert_results_to_state_changes(&results, 1)
                .expect("convert ok");
            let env = setu_types::ObjectEnvelope::from_bytes(
                changes[0].new_state.as_ref().unwrap(),
            )
            .unwrap();
            let pl: DfFieldValue = bcs::from_bytes(&env.data).unwrap();
            assert_eq!(pl.name_bcs, raw_key);
            assert_eq!(pl.value_bcs, raw_val);
            assert_eq!(
                env.type_tag,
                "0x1::dynamic_field::Field<vector<u8>, vector<u8>>"
            );
        }
    }
}
