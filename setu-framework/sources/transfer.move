// ===== setu-framework/sources/transfer.move =====
// Ownership transfer operations. All functions delegate to native implementations.
// Native functions implemented in setu-move-vm/src/natives.rs
module setu::transfer {
    /// Transfer an owned object to a new owner
    public fun transfer<T: key>(obj: T, recipient: address) {
        transfer_internal(obj, recipient);
    }

    /// Make an object publicly shared (Phase 0-4: aborts with E_SHARED_NOT_SUPPORTED = 1001)
    /// ADR-1: Shared Object not supported until Phase 5+
    public fun share_object<T: key>(obj: T) {
        share_internal(obj);
    }

    /// Freeze an object as immutable
    /// After freezing, everyone can read but no one can write
    public fun freeze_object<T: key>(obj: T) {
        freeze_internal(obj);
    }

    // ── Native functions (implemented in natives.rs §9) ──
    native fun transfer_internal<T: key>(obj: T, recipient: address);
    native fun share_internal<T: key>(obj: T);
    native fun freeze_internal<T: key>(obj: T);
}
