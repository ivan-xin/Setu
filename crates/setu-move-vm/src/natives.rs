//! Native function implementations for Setu stdlib.
//!
//! 7 natives registered under `0x1`:
//! - object::new_uid_internal
//! - object::delete_uid_internal
//! - object::uid_to_address_internal
//! - transfer::transfer_internal
//! - transfer::share_internal
//! - transfer::freeze_internal
//! - tx_context::derive_id_internal

use std::collections::VecDeque;
use std::sync::Arc;

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress,
    gas_algebra::InternalGas,
    identifier::Identifier,
    vm_status::StatusCode,
};
use move_vm_runtime::native_functions::{NativeContext, NativeFunction, NativeFunctionTable};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    values::{Struct, Value},
};
use smallvec::smallvec;

use setu_types::object::ObjectId;

use crate::object_runtime::SetuObjectRuntime;

// ═══════════════════════════════════════════════════════════════
// Registration table
// ═══════════════════════════════════════════════════════════════

/// Build the native function table for `0x1` (setu framework address).
pub fn setu_native_functions() -> NativeFunctionTable {
    let addr = AccountAddress::ONE;
    make_table(
        addr,
        &[
            ("object", "new_uid_internal", native_object_new_uid),
            ("object", "delete_uid_internal", native_object_delete_uid),
            (
                "object",
                "uid_to_address_internal",
                native_uid_to_address,
            ),
            ("transfer", "transfer_internal", native_transfer_internal),
            ("transfer", "share_internal", native_share_internal),
            ("transfer", "freeze_internal", native_freeze_internal),
            ("tx_context", "derive_id_internal", native_derive_id),
        ],
    )
}

type RawNativeFn = fn(
    &mut NativeContext,
    Vec<Type>,
    VecDeque<Value>,
) -> PartialVMResult<NativeResult>;

fn make_table(
    addr: AccountAddress,
    entries: &[(&str, &str, RawNativeFn)],
) -> NativeFunctionTable {
    entries
        .iter()
        .map(|(module, func, f)| {
            (
                addr,
                Identifier::new(*module).expect("valid identifier"),
                Identifier::new(*func).expect("valid identifier"),
                Arc::new(*f) as NativeFunction,
            )
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════
// object::new_uid_internal
// ═══════════════════════════════════════════════════════════════

/// Move: `native fun new_uid_internal(ctx: &mut TxContext): ID;`
///
/// Generates a deterministic object ID via SetuObjectRuntime.fresh_id().
/// Must consume the `&mut TxContext` arg to maintain VM stack balance (R4-C1).
fn native_object_new_uid(
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    // Consume &mut TxContext (unused — ID generation via SetuObjectRuntime)
    let _ctx = args
        .pop_back()
        .ok_or_else(|| PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))?;

    let runtime = context.extensions_mut().get_mut::<SetuObjectRuntime>()?;
    let new_id = runtime.fresh_id();

    // ID { bytes: address }
    let id_value = Value::address(AccountAddress::new(*new_id.as_bytes()));
    let id_struct = Value::struct_(Struct::pack(vec![id_value]));

    Ok(NativeResult::ok(InternalGas::zero(), smallvec![id_struct]))
}

// ═══════════════════════════════════════════════════════════════
// object::delete_uid_internal
// ═══════════════════════════════════════════════════════════════

/// Move: `native fun delete_uid_internal(id: ID);`
///
/// If the ID was created this tx, removes from `created_ids`.
/// Otherwise marks as deleted in `deleted_ids`.
fn native_object_delete_uid(
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let id_value = args
        .pop_back()
        .ok_or_else(|| PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))?;

    // ID { bytes: address } — BCS layout: struct with one address field
    let layout = move_vm_types::values::VMValueCast::cast(id_value)?;
    let id_struct: Struct = layout;
    let fields: Vec<Value> = id_struct.unpack()?.collect();
    if fields.is_empty() {
        return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR));
    }
    let addr: AccountAddress = move_vm_types::values::VMValueCast::cast(fields.into_iter().next().unwrap())?;
    let object_id = ObjectId::new(addr.into_bytes());

    let runtime = context.extensions_mut().get_mut::<SetuObjectRuntime>()?;
    if !runtime.created_ids.swap_remove(&object_id) {
        runtime.delete_object(object_id);
    }

    Ok(NativeResult::ok(InternalGas::zero(), smallvec![]))
}

// ═══════════════════════════════════════════════════════════════
// object::uid_to_address_internal
// ═══════════════════════════════════════════════════════════════

/// Move: `native fun uid_to_address_internal(uid: &UID): address;`
///
/// UID { id: ID { bytes: address } } → extracts the inner address.
fn native_uid_to_address(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let uid_val = args
        .pop_back()
        .ok_or_else(|| PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))?;

    // UID layout in BCS: Struct { id: ID { bytes: address } }
    // Front 32 bytes = the address.
    use move_vm_types::values::VMValueCast;
    let uid_struct: Struct = VMValueCast::cast(uid_val)?;
    let mut uid_fields: Vec<Value> = uid_struct.unpack()?.collect();
    if uid_fields.is_empty() {
        return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR));
    }
    let id_val = uid_fields.remove(0);
    let id_struct: Struct = VMValueCast::cast(id_val)?;
    let mut id_fields: Vec<Value> = id_struct.unpack()?.collect();
    if id_fields.is_empty() {
        return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR));
    }
    let addr: AccountAddress = VMValueCast::cast(id_fields.remove(0))?;

    Ok(NativeResult::ok(
        InternalGas::zero(),
        smallvec![Value::address(addr)],
    ))
}

// ═══════════════════════════════════════════════════════════════
// transfer::transfer_internal<T>
// ═══════════════════════════════════════════════════════════════

/// Move: `native fun transfer_internal<T: key>(obj: T, recipient: address);`
fn native_transfer_internal(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    // Args: (obj: T, recipient: address)  — pop in reverse order
    let recipient: AccountAddress =
        move_vm_types::values::VMValueCast::cast(args.pop_back().ok_or_else(|| {
            PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
        })?)?;
    let obj_value = args.pop_back().ok_or_else(|| {
        PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
    })?;
    let obj_type = ty_args.pop().ok_or_else(|| {
        PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
    })?;

    // Type tag
    let type_tag = context.type_to_type_tag(&obj_type)?;
    let struct_tag = match type_tag {
        move_core_types::language_storage::TypeTag::Struct(st) => *st,
        _ => return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)),
    };

    // BCS serialize
    let layout = context
        .type_to_type_layout(&obj_type)?
        .ok_or_else(|| PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))?;
    let bcs_bytes = obj_value
        .typed_serialize(&layout)
        .ok_or_else(|| PartialVMError::new(StatusCode::VALUE_SERIALIZATION_ERROR))?;

    // Extract UID from first 32 bytes
    let object_id = extract_uid_from_bcs(&bcs_bytes)?;

    // Record transfer
    let runtime = context.extensions_mut().get_mut::<SetuObjectRuntime>()?;
    runtime.transfer_object(object_id, recipient, struct_tag, bcs_bytes);

    Ok(NativeResult::ok(InternalGas::zero(), smallvec![]))
}

// ═══════════════════════════════════════════════════════════════
// transfer::share_internal<T>
// ═══════════════════════════════════════════════════════════════

/// Move: `native fun share_internal<T: key>(obj: T);`
///
/// Phase 0-4: Shared objects not supported (ADR-1). Aborts with code 1001.
fn native_share_internal(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    Ok(NativeResult::err(
        InternalGas::zero(),
        /* E_SHARED_NOT_SUPPORTED */ 1001,
    ))
}

// ═══════════════════════════════════════════════════════════════
// transfer::freeze_internal<T>
// ═══════════════════════════════════════════════════════════════

/// Move: `native fun freeze_internal<T: key>(obj: T);`
fn native_freeze_internal(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let obj_value = args.pop_back().ok_or_else(|| {
        PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
    })?;
    let obj_type = ty_args.pop().ok_or_else(|| {
        PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
    })?;

    let type_tag = context.type_to_type_tag(&obj_type)?;
    let struct_tag = match type_tag {
        move_core_types::language_storage::TypeTag::Struct(st) => *st,
        _ => return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)),
    };

    let layout = context
        .type_to_type_layout(&obj_type)?
        .ok_or_else(|| PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))?;
    let bcs_bytes = obj_value
        .typed_serialize(&layout)
        .ok_or_else(|| PartialVMError::new(StatusCode::VALUE_SERIALIZATION_ERROR))?;

    let object_id = extract_uid_from_bcs(&bcs_bytes)?;

    let runtime = context.extensions_mut().get_mut::<SetuObjectRuntime>()?;
    runtime.freeze_object(object_id, struct_tag, bcs_bytes);

    Ok(NativeResult::ok(InternalGas::zero(), smallvec![]))
}

// ═══════════════════════════════════════════════════════════════
// tx_context::derive_id_internal
// ═══════════════════════════════════════════════════════════════

/// Move: `native fun derive_id(tx_hash: vector<u8>, creation_num: u64): address;`
fn native_derive_id(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    use move_vm_types::values::VMValueCast;
    let creation_num: u64 = VMValueCast::cast(
        args.pop_back()
            .ok_or_else(|| PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))?,
    )?;
    let tx_hash_val: Vec<u8> = VMValueCast::cast(
        args.pop_back()
            .ok_or_else(|| PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR))?,
    )?;

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"SETU_DERIVE_ID:");
    hasher.update(&tx_hash_val);
    hasher.update(&creation_num.to_le_bytes());
    let hash = hasher.finalize();

    let addr = AccountAddress::new(*hash.as_bytes());
    Ok(NativeResult::ok(
        InternalGas::zero(),
        smallvec![Value::address(addr)],
    ))
}

// ═══════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════

/// Extract object UID from BCS bytes.
///
/// Layout: `Struct { id: UID { id: ID { bytes: address } }, ... }`
/// First 32 bytes = the address (all nested structs start at offset 0 in BCS).
fn extract_uid_from_bcs(bcs_bytes: &[u8]) -> PartialVMResult<ObjectId> {
    if bcs_bytes.len() < 32 {
        return Err(PartialVMError::new(
            StatusCode::VALUE_DESERIALIZATION_ERROR,
        ));
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&bcs_bytes[..32]);
    Ok(ObjectId::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_table_count() {
        let table = setu_native_functions();
        assert_eq!(table.len(), 7);
    }

    #[test]
    fn test_make_table_modules() {
        let table = setu_native_functions();
        let modules: Vec<String> = table.iter().map(|(_, m, _, _)| m.to_string()).collect();
        assert!(modules.contains(&"object".to_string()));
        assert!(modules.contains(&"transfer".to_string()));
        assert!(modules.contains(&"tx_context".to_string()));
    }

    #[test]
    fn test_extract_uid_from_bcs() {
        let mut data = vec![0u8; 64];
        data[..32].copy_from_slice(&[0x42; 32]);
        let id = extract_uid_from_bcs(&data).unwrap();
        assert_eq!(id, ObjectId::new([0x42; 32]));
    }

    #[test]
    fn test_extract_uid_too_short() {
        let data = vec![0u8; 16];
        assert!(extract_uid_from_bcs(&data).is_err());
    }

    #[test]
    fn test_derive_id_deterministic() {
        // Simulate derive_id logic directly
        let tx_hash = vec![0xAA; 32];
        let creation_num: u64 = 5;

        let mut h1 = blake3::Hasher::new();
        h1.update(b"SETU_DERIVE_ID:");
        h1.update(&tx_hash);
        h1.update(&creation_num.to_le_bytes());
        let r1 = h1.finalize();

        let mut h2 = blake3::Hasher::new();
        h2.update(b"SETU_DERIVE_ID:");
        h2.update(&tx_hash);
        h2.update(&creation_num.to_le_bytes());
        let r2 = h2.finalize();

        assert_eq!(r1, r2);
    }
}
