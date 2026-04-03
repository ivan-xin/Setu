//! Resource types for the three-resource economic model: SETU / Flux / Power
//!
//! - SETU: transferable token (existing Coin type, BCS serialized)
//! - Flux: non-transferable credit score (JSON serialized)
//! - Power: non-transferable life counter (JSON serialized)

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
use crate::ObjectId;
use crate::hash_utils::setu_hash_with_domain;

// ========== Constants ==========

/// Initial Power for new accounts (21 million events lifetime)
pub const INITIAL_POWER: u64 = 21_000_000;

/// Initial Flux for new accounts
pub const INITIAL_FLUX: u64 = 0;

// ========== FluxState ==========

/// Flux (credit score) stored as independent object in Merkle tree (JSON serialized).
///
/// State key: `"oid:{hex}"` where hex = BLAKE3("SETU_FLUX:" || address)
///
/// Per-address (global), NOT per-subnet. One FluxState per user regardless of
/// how many subnets they're registered in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FluxState {
    /// User address (hex string)
    pub address: String,
    /// Flux score — increases via successful solver-executed events
    pub flux: u64,
    /// Last activity timestamp (deterministic, from ExecutionContext.timestamp)
    pub last_active_at: u64,
    /// Version (incremented on each write)
    pub version: u64,
}

impl FluxState {
    /// Create a new FluxState for a newly registered user.
    pub fn new(address: &str, timestamp: u64) -> Self {
        Self {
            address: address.to_string(),
            flux: INITIAL_FLUX,
            last_active_at: timestamp,
            version: 0,
        }
    }
}

// ========== PowerState ==========

/// Power (life counter) stored as independent object in Merkle tree (JSON serialized).
///
/// State key: `"oid:{hex}"` where hex = BLAKE3("SETU_POWER:" || address)
///
/// Per-address (global), NOT per-subnet. One PowerState per user regardless of
/// how many subnets they're registered in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PowerState {
    /// User address (hex string)
    pub address: String,
    /// Remaining Power (starts at INITIAL_POWER, decremented by 1 per solver-executed event)
    pub power_remaining: u64,
    /// Version (incremented on each write)
    pub version: u64,
}

impl PowerState {
    /// Create a new PowerState for a newly registered user.
    pub fn new(address: &str) -> Self {
        Self {
            address: address.to_string(),
            power_remaining: INITIAL_POWER,
            version: 0,
        }
    }
}

// ========== Governance ==========

/// Resource governance mode (pre-submission only, never affects deterministic executor).
///
/// - `Enabled`: enforce admission checks (freeze check for Power, score check for Flux)
/// - `Disabled`: skip admission checks (default — allows gradual rollout)
/// - `DryRun`: compute and log checks but don't enforce
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum ResourceGovernanceMode {
    Enabled = 0,
    #[default]
    Disabled = 1,
    DryRun = 2,
}

/// Atomic wrapper for ResourceGovernanceMode (thread-safe, lock-free).
///
/// Node-local configuration — different validators can have different settings.
/// This is safe because governance only affects pre-submission admission checks,
/// never the deterministic executor.
pub struct AtomicGovernanceMode(AtomicU8);

impl AtomicGovernanceMode {
    pub fn new(mode: ResourceGovernanceMode) -> Self {
        Self(AtomicU8::new(mode as u8))
    }

    pub fn load(&self) -> ResourceGovernanceMode {
        match self.0.load(Ordering::Relaxed) {
            0 => ResourceGovernanceMode::Enabled,
            2 => ResourceGovernanceMode::DryRun,
            _ => ResourceGovernanceMode::Disabled,
        }
    }

    pub fn store(&self, mode: ResourceGovernanceMode) {
        self.0.store(mode as u8, Ordering::Relaxed);
    }
}

impl Default for AtomicGovernanceMode {
    fn default() -> Self {
        Self::new(ResourceGovernanceMode::default())
    }
}

// ========== Deterministic ObjectId Helpers ==========

/// Compute deterministic ObjectId for a user's FluxState.
///
/// Key: BLAKE3("SETU_FLUX:" || canonical_address)
/// Address is lowercased to ensure canonical form (R8-ISSUE-1).
/// Stored as: `"oid:{hex}"` (G11 compliant)
pub fn flux_state_object_id(address: &str) -> ObjectId {
    let canonical = address.to_ascii_lowercase();
    ObjectId::new(setu_hash_with_domain(b"SETU_FLUX:", canonical.as_bytes()))
}

/// Compute deterministic ObjectId for a user's PowerState.
///
/// Key: BLAKE3("SETU_POWER:" || canonical_address)
/// Address is lowercased to ensure canonical form (R8-ISSUE-1).
/// Stored as: `"oid:{hex}"` (G11 compliant)
pub fn power_state_object_id(address: &str) -> ObjectId {
    let canonical = address.to_ascii_lowercase();
    ObjectId::new(setu_hash_with_domain(b"SETU_POWER:", canonical.as_bytes()))
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flux_state_new() {
        let fs = FluxState::new("0xabc", 1000);
        assert_eq!(fs.flux, 0);
        assert_eq!(fs.last_active_at, 1000);
        assert_eq!(fs.version, 0);
    }

    #[test]
    fn test_power_state_new() {
        let ps = PowerState::new("0xabc");
        assert_eq!(ps.power_remaining, INITIAL_POWER);
        assert_eq!(ps.version, 0);
    }

    #[test]
    fn test_flux_state_json_roundtrip() {
        let fs = FluxState::new("0xabc", 1000);
        let json = serde_json::to_vec(&fs).unwrap();
        let fs2: FluxState = serde_json::from_slice(&json).unwrap();
        assert_eq!(fs, fs2);
    }

    #[test]
    fn test_power_state_json_roundtrip() {
        let ps = PowerState::new("0xdef");
        let json = serde_json::to_vec(&ps).unwrap();
        let ps2: PowerState = serde_json::from_slice(&json).unwrap();
        assert_eq!(ps, ps2);
    }

    #[test]
    fn test_object_ids_deterministic() {
        let id1 = flux_state_object_id("0xabc");
        let id2 = flux_state_object_id("0xabc");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_object_ids_differ_by_type() {
        let flux_id = flux_state_object_id("0xabc");
        let power_id = power_state_object_id("0xabc");
        assert_ne!(flux_id, power_id);
    }

    #[test]
    fn test_governance_default_disabled() {
        let mode = ResourceGovernanceMode::default();
        assert_eq!(mode, ResourceGovernanceMode::Disabled);
    }

    #[test]
    fn test_atomic_governance_mode() {
        let agm = AtomicGovernanceMode::default();
        assert_eq!(agm.load(), ResourceGovernanceMode::Disabled);
        agm.store(ResourceGovernanceMode::Enabled);
        assert_eq!(agm.load(), ResourceGovernanceMode::Enabled);
        agm.store(ResourceGovernanceMode::DryRun);
        assert_eq!(agm.load(), ResourceGovernanceMode::DryRun);
    }
}
