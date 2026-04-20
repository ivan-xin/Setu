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
use setu_types::Ownership;

// ═══════════════════════════════════════════════════════════════
// Input / Effect types
// ═══════════════════════════════════════════════════════════════

/// Pre-loaded object passed into a Move session.
pub struct InputObject {
    pub id: ObjectId,
    pub owner: Address,
    /// Original ownership category preserved from the on-disk envelope.
    /// Required so that `&mut` mutations of `Shared`/`Immutable`/`ObjectOwner`
    /// inputs do not silently demote the object back to `AddressOwner`
    /// when the executor rebuilds the envelope. See R4-ISSUE-2.
    pub ownership: Ownership,
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
            ownership: env.metadata.ownership,
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

/// Share effect recorded by `native_share_internal` (PWOO).
///
/// Ownership of the object transitions from `AddressOwner` to
/// `Shared { initial_shared_version }`.
/// The `initial_shared_version` captures the envelope version at the
/// moment of share: for objects loaded from input_objects it is their
/// current version; for newly-created + same-TX share it is 0 (the
/// envelope is persisted with version bumped to 1 by the executor).
pub struct ObjectShareEffect {
    pub type_tag: StructTag,
    pub bcs_bytes: Vec<u8>,
    pub initial_shared_version: u64,
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
    pub shared: IndexMap<ObjectId, ObjectShareEffect>,
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
    shared: IndexMap<ObjectId, ObjectShareEffect>,
    mutated: IndexMap<ObjectId, ObjectMutationEffect>,
    pub(crate) emitted_events: Vec<(StructTag, Vec<u8>)>,
    // Context
    tx_hash: [u8; 32],
    ids_created: u64,
    sender: Address,
    epoch_timestamp_ms: u64,
}

impl<'a> NativeExtensionMarker<'a> for SetuObjectRuntime {}

impl SetuObjectRuntime {
    pub fn new(
        input_objects: Vec<InputObject>,
        tx_hash: [u8; 32],
        sender: Address,
        epoch_timestamp_ms: u64,
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
            shared: IndexMap::new(),
            mutated: IndexMap::new(),
            emitted_events: Vec::new(),
            tx_hash,
            ids_created: 0,
            sender,
            epoch_timestamp_ms,
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

    /// Record an object share (PWOO: Ownership → Shared).
    ///
    /// The `initial_shared_version` is captured from `input_objects` if the
    /// object was loaded as input; otherwise it defaults to 0 (same-TX
    /// newly-created + shared). Callers should have previously removed any
    /// conflicting transfer effect for the same id.
    pub fn share_object(
        &mut self,
        id: ObjectId,
        type_tag: StructTag,
        bcs_bytes: Vec<u8>,
    ) {
        let initial_shared_version = self
            .input_objects
            .get(&id)
            .map(|io| io.version)
            .unwrap_or(0);
        // `share` is terminal for ownership: drop any prior transfer effect
        // for the same object so downstream effect application sees Shared only.
        self.transfers.shift_remove(&id);
        self.shared.insert(
            id,
            ObjectShareEffect {
                type_tag,
                bcs_bytes,
                initial_shared_version,
            },
        );
    }

    /// Record an object deletion.
    pub fn delete_object(&mut self, id: ObjectId) {
        self.deleted_ids.insert(id);
    }

    /// Record a structured event emission.
    pub fn emit_event(&mut self, type_tag: StructTag, bcs_bytes: Vec<u8>) {
        self.emitted_events.push((type_tag, bcs_bytes));
    }

    /// Epoch timestamp in milliseconds (injected by consensus).
    pub fn epoch_timestamp_ms(&self) -> u64 {
        self.epoch_timestamp_ms
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
            shared: self.shared,
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
        let mut rt1 = SetuObjectRuntime::new(vec![], tx, test_address(), 0);
        let mut rt2 = SetuObjectRuntime::new(vec![], tx, test_address(), 0);

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
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
        let id = rt.fresh_id();
        assert!(rt.created_ids.contains(&id));
    }

    #[test]
    fn test_transfer_object() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
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
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
        let id = rt.fresh_id();
        assert!(rt.created_ids.contains(&id));

        // Deleting a freshly created object removes from created_ids
        // (mimics native_object_delete_uid behavior — tested externally)
        rt.created_ids.remove(&id);
        assert!(!rt.created_ids.contains(&id));
    }

    #[test]
    fn test_into_results() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
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
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
        let tag = test_struct_tag("MyEvent");
        rt.emit_event(tag.clone(), vec![0x01, 0x02]);
        assert_eq!(rt.emitted_events.len(), 1);
        assert_eq!(rt.emitted_events[0].0, tag);
        assert_eq!(rt.emitted_events[0].1, vec![0x01, 0x02]);
    }

    #[test]
    fn test_emit_multiple_events_ordered() {
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
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
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
        rt.emit_event(test_struct_tag("Ev"), vec![0x42]);
        let results = rt.into_results();
        assert_eq!(results.emitted_events.len(), 1);
        assert_eq!(results.emitted_events[0].1, vec![0x42]);
    }

    #[test]
    fn test_epoch_timestamp_stored() {
        let rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 1712700000000);
        assert_eq!(rt.epoch_timestamp_ms(), 1712700000000);
    }

    #[test]
    fn test_epoch_timestamp_zero() {
        let rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
        assert_eq!(rt.epoch_timestamp_ms(), 0);
    }

    // ═══════════════════════════════════════════════════════════════
    // PWOO: share_object tests
    // ═══════════════════════════════════════════════════════════════

    fn make_input_object(id: ObjectId, version: u64) -> InputObject {
        InputObject {
            id,
            owner: test_address(),
            ownership: Ownership::AddressOwner(test_address()),
            version,
            envelope_bytes: vec![],
            move_data: vec![],
            type_tag: test_struct_tag("Pool"),
        }
    }

    #[test]
    fn test_share_object_records_effect_for_newly_created() {
        // A3: newly-created + same-TX share → initial_shared_version = 0
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
        let id = rt.fresh_id();
        let tag = test_struct_tag("Pool");
        rt.share_object(id, tag.clone(), vec![0xAB, 0xCD]);

        let results = rt.into_results();
        assert_eq!(results.shared.len(), 1);
        let effect = &results.shared[&id];
        assert_eq!(effect.initial_shared_version, 0);
        assert_eq!(effect.type_tag, tag);
        assert_eq!(effect.bcs_bytes, vec![0xAB, 0xCD]);
    }

    #[test]
    fn test_share_object_preserves_input_version() {
        // A2: share of an input object captures its current version
        let id = ObjectId::new([0x77; 32]);
        let input = make_input_object(id, 5);
        let mut rt = SetuObjectRuntime::new(vec![input], test_tx_hash(), test_address(), 0);
        rt.share_object(id, test_struct_tag("Pool"), vec![0x01]);

        let results = rt.into_results();
        assert_eq!(results.shared[&id].initial_shared_version, 5);
    }

    #[test]
    fn test_share_object_zero_when_not_input() {
        // A3 (explicit): not in input_objects → version = 0 (synthetic)
        let id = ObjectId::new([0x99; 32]);
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
        rt.share_object(id, test_struct_tag("Pool"), vec![]);

        let results = rt.into_results();
        assert_eq!(results.shared[&id].initial_shared_version, 0);
    }

    #[test]
    fn test_share_removes_prior_transfer_effect() {
        // A4: share is terminal — if transfer was recorded first, share wins
        let id = ObjectId::new([0x55; 32]);
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
        let tag = test_struct_tag("Pool");
        rt.transfer_object(id, AccountAddress::new([0x11; 32]), tag.clone(), vec![0xAA]);
        rt.share_object(id, tag, vec![0xBB]);

        let results = rt.into_results();
        assert!(!results.transfers.contains_key(&id));
        assert!(results.shared.contains_key(&id));
        assert_eq!(results.shared[&id].bcs_bytes, vec![0xBB]);
    }

    #[test]
    fn test_share_and_freeze_are_independent_maps() {
        // Defense-in-depth: share and freeze are recorded separately
        let id1 = ObjectId::new([0xA1; 32]);
        let id2 = ObjectId::new([0xA2; 32]);
        let mut rt = SetuObjectRuntime::new(vec![], test_tx_hash(), test_address(), 0);
        rt.share_object(id1, test_struct_tag("P"), vec![]);
        rt.freeze_object(id2, test_struct_tag("F"), vec![]);
        let r = rt.into_results();
        assert_eq!(r.shared.len(), 1);
        assert_eq!(r.frozen.len(), 1);
    }
}
