//! SetuObjectRuntime — injected into NativeContextExtensions for Move sessions.
//!
//! Tracks object creates, transfers, mutations, freezes, and deletions
//! during a single Move transaction execution.

use std::collections::BTreeMap;

use better_any::{Tid, TidAble};
use indexmap::{IndexMap, IndexSet};
use move_core_types::{account_address::AccountAddress, language_storage::StructTag};
use move_vm_runtime::native_extensions::NativeExtensionMarker;

use setu_types::object::{Address, ObjectId};

// ═══════════════════════════════════════════════════════════════
// Input / Effect types
// ═══════════════════════════════════════════════════════════════

/// Pre-loaded object passed into a Move session.
pub struct InputObject {
    pub id: ObjectId,
    pub owner: Address,
    pub version: u64,
    /// Full ObjectEnvelope BCS (for old_state in StateChange)
    pub envelope_bytes: Vec<u8>,
    /// Inner Move struct BCS (for VM consumption)
    pub move_data: Vec<u8>,
    pub type_tag: StructTag,
}

impl InputObject {
    /// Construct an InputObject from an ObjectEnvelope.
    ///
    /// Parses the envelope's type_tag string into a StructTag.
    /// Returns error if parsing fails (malformed data, not a silent skip).
    pub fn from_envelope(
        id: &ObjectId,
        env: &setu_types::ObjectEnvelope,
    ) -> Result<Self, setu_runtime::error::RuntimeError> {
        use std::str::FromStr;
        let type_tag = StructTag::from_str(&env.type_tag).map_err(|e| {
            setu_runtime::error::RuntimeError::InvalidTransaction(format!(
                "Failed to parse type_tag '{}': {}",
                env.type_tag, e
            ))
        })?;
        Ok(Self {
            id: *id,
            owner: env.metadata.owner,
            version: env.metadata.version,
            envelope_bytes: env.to_bytes(),
            move_data: env.data.clone(),
            type_tag,
        })
    }
}

/// Transfer effect recorded by `native_transfer_internal`.
pub struct ObjectTransferEffect {
    pub new_owner: AccountAddress,
    pub type_tag: StructTag,
    pub bcs_bytes: Vec<u8>,
}

/// Freeze effect recorded by `native_freeze_internal`.
pub struct ObjectFreezeEffect {
    pub type_tag: StructTag,
    pub bcs_bytes: Vec<u8>,
}

/// Mutation effect recorded for `&mut` objects.
pub struct ObjectMutationEffect {
    pub type_tag: StructTag,
    pub bcs_bytes: Vec<u8>,
}

/// Aggregated results after a session finishes.
pub struct ObjectRuntimeResults {
    pub input_objects: BTreeMap<ObjectId, InputObject>,
    pub created_ids: IndexSet<ObjectId>,
    pub deleted_ids: IndexSet<ObjectId>,
    pub transfers: IndexMap<ObjectId, ObjectTransferEffect>,
    pub frozen: IndexMap<ObjectId, ObjectFreezeEffect>,
    pub mutated: IndexMap<ObjectId, ObjectMutationEffect>,
    pub emitted_events: Vec<(StructTag, Vec<u8>)>,
}

// ═══════════════════════════════════════════════════════════════
// SetuObjectRuntime
// ═══════════════════════════════════════════════════════════════

/// Runtime object tracker — injected as NativeContextExtension.
///
/// Natives access this via `context.extensions_mut().get_mut::<SetuObjectRuntime>()`.
/// Lifetime: one session execution.
#[derive(Tid)]
pub struct SetuObjectRuntime {
    // Input
    input_objects: BTreeMap<ObjectId, InputObject>,
    // Accumulated effects
    pub(crate) created_ids: IndexSet<ObjectId>,
    deleted_ids: IndexSet<ObjectId>,
    transfers: IndexMap<ObjectId, ObjectTransferEffect>,
    frozen: IndexMap<ObjectId, ObjectFreezeEffect>,
    mutated: IndexMap<ObjectId, ObjectMutationEffect>,
    pub(crate) emitted_events: Vec<(StructTag, Vec<u8>)>,
    // Context
    tx_hash: [u8; 32],
    ids_created: u64,
    sender: Address,
}

impl<'a> NativeExtensionMarker<'a> for SetuObjectRuntime {}

impl SetuObjectRuntime {
    pub fn new(
        input_objects: Vec<InputObject>,
        tx_hash: [u8; 32],
        sender: Address,
    ) -> Self {
        let input_map = input_objects
            .into_iter()
            .map(|obj| (obj.id, obj))
            .collect();
        Self {
            input_objects: input_map,
            created_ids: IndexSet::new(),
            deleted_ids: IndexSet::new(),
            transfers: IndexMap::new(),
            frozen: IndexMap::new(),
            mutated: IndexMap::new(),
            emitted_events: Vec::new(),
            tx_hash,
            ids_created: 0,
            sender,
        }
    }

    /// Generate a deterministic object ID.
    /// ID = BLAKE3("SETU_FRESH_ID:" || tx_hash || ids_created.to_le_bytes())
    pub fn fresh_id(&mut self) -> ObjectId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"SETU_FRESH_ID:");
        hasher.update(&self.tx_hash);
        hasher.update(&self.ids_created.to_le_bytes());
        let hash = hasher.finalize();
        self.ids_created += 1;

        let id = ObjectId::new(*hash.as_bytes());
        self.created_ids.insert(id);
        id
    }

    /// Record an object transfer.
    pub fn transfer_object(
        &mut self,
        id: ObjectId,
        new_owner: AccountAddress,
        type_tag: StructTag,
        bcs_bytes: Vec<u8>,
    ) {
        self.transfers.insert(
            id,
            ObjectTransferEffect {
                new_owner,
                type_tag,
                bcs_bytes,
            },
        );
    }

    /// Record an object mutation.
    pub fn mutate_object(
        &mut self,
        id: ObjectId,
        type_tag: StructTag,
        bcs_bytes: Vec<u8>,
    ) {
        self.mutated.insert(id, ObjectMutationEffect { type_tag, bcs_bytes });
    }

    /// Record an object freeze (Ownership → Immutable).
    pub fn freeze_object(
        &mut self,
        id: ObjectId,
        type_tag: StructTag,
        bcs_bytes: Vec<u8>,
    ) {
        self.frozen.insert(id, ObjectFreezeEffect { type_tag, bcs_bytes });
    }

    /// Record an object deletion.
    pub fn delete_object(&mut self, id: ObjectId) {
        self.deleted_ids.insert(id);
    }

    /// Record a structured event emission.
    pub fn emit_event(&mut self, type_tag: StructTag, bcs_bytes: Vec<u8>) {
        self.emitted_events.push((type_tag, bcs_bytes));
    }

    /// Look up a pre-loaded input object.
    pub fn get_input_object(&self, id: &ObjectId) -> Option<&InputObject> {
        self.input_objects.get(id)
    }

    /// Sender address for this transaction.
    pub fn sender(&self) -> &Address {
        &self.sender
    }

    /// Consume self and return aggregated results.
    pub fn into_results(self) -> ObjectRuntimeResults {
        ObjectRuntimeResults {
            input_objects: self.input_objects,
            created_ids: self.created_ids,
            deleted_ids: self.deleted_ids,
            transfers: self.transfers,
            frozen: self.frozen,
            mutated: self.mutated,
            emitted_events: self.emitted_events,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_address() -> Address {
        Address::new([0xAA; 32])
    }

    fn test_tx_hash() -> [u8; 32] {
        [0xBB; 32]
    }

    #[test]
    fn test_fresh_id_deterministic() {
        let tx = test_tx_hash();
        let mut rt1 = SetuObjectRuntime::new(vec![], tx, test_address());
        let mut rt2 = SetuObjectRuntime::new(vec![], tx, test_address());

        let id1a = rt1.fresh_id();
        let id1b = rt1.fresh_id();
        let id2a = rt2.fresh_id();
        let id2b = rt2.fresh_id();

        assert_eq!(id1a, id2a, "same tx_hash + counter → same ID");
        assert_eq!(id1b, id2b);
        assert_ne!(id1a, id1b, "different counters → different IDs");
    }

    #[test]
    fn test_fresh_id_tracked_as_created() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address());
        let id = rt.fresh_id();
        assert!(rt.created_ids.contains(&id));
    }

    #[test]
    fn test_transfer_object() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address());
        let id = ObjectId::new([0x01; 32]);
        let recipient = AccountAddress::new([0x02; 32]);
        let tag = StructTag {
            address: AccountAddress::ONE,
            module: move_core_types::identifier::Identifier::new("coin").unwrap(),
            name: move_core_types::identifier::Identifier::new("Coin").unwrap(),
            type_params: vec![],
        };
        rt.transfer_object(id, recipient, tag, vec![0xDE, 0xAD]);

        let results = rt.into_results();
        assert_eq!(results.transfers.len(), 1);
        assert_eq!(results.transfers[&id].new_owner, recipient);
    }

    #[test]
    fn test_delete_created_is_noop() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address());
        let id = rt.fresh_id();
        assert!(rt.created_ids.contains(&id));

        // Deleting a freshly created object removes from created_ids
        // (mimics native_object_delete_uid behavior — tested externally)
        rt.created_ids.remove(&id);
        assert!(!rt.created_ids.contains(&id));
    }

    #[test]
    fn test_into_results() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address());
        let _ = rt.fresh_id();
        rt.delete_object(ObjectId::new([0xFF; 32]));

        let results = rt.into_results();
        assert_eq!(results.created_ids.len(), 1);
        assert_eq!(results.deleted_ids.len(), 1);
    }

    fn test_struct_tag(name: &str) -> StructTag {
        StructTag {
            address: AccountAddress::ONE,
            module: move_core_types::identifier::Identifier::new("test").unwrap(),
            name: move_core_types::identifier::Identifier::new(name).unwrap(),
            type_params: vec![],
        }
    }

    #[test]
    fn test_emit_event_recorded() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address());
        let tag = test_struct_tag("MyEvent");
        rt.emit_event(tag.clone(), vec![0x01, 0x02]);
        assert_eq!(rt.emitted_events.len(), 1);
        assert_eq!(rt.emitted_events[0].0, tag);
        assert_eq!(rt.emitted_events[0].1, vec![0x01, 0x02]);
    }

    #[test]
    fn test_emit_multiple_events_ordered() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address());
        let tag_a = test_struct_tag("EventA");
        let tag_b = test_struct_tag("EventB");
        rt.emit_event(tag_a.clone(), vec![0xAA]);
        rt.emit_event(tag_b.clone(), vec![0xBB]);
        rt.emit_event(tag_a.clone(), vec![0xCC]);

        assert_eq!(rt.emitted_events.len(), 3);
        assert_eq!(rt.emitted_events[0].0, tag_a);
        assert_eq!(rt.emitted_events[1].0, tag_b);
        assert_eq!(rt.emitted_events[2].0, tag_a);
    }

    #[test]
    fn test_emit_events_in_results() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address());
        rt.emit_event(test_struct_tag("Ev"), vec![0x42]);
        let results = rt.into_results();
        assert_eq!(results.emitted_events.len(), 1);
        assert_eq!(results.emitted_events[0].1, vec![0x42]);
    }
}
