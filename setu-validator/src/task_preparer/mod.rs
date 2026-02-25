//! SolverTask Preparation Module (solver-tee3 Architecture)
//!
//! This module handles the preparation of SolverTask for Solver execution.
//! According to solver-tee3 design:
//!
//! - **Validator prepares everything**: coin selection, read_set, Merkle proofs
//! - **Solver is pass-through**: receives SolverTask, passes to TEE
//! - **TEE validates and executes**: verifies proofs, executes STF
//!
//! ## Components
//!
//! - [`TaskPreparer`]: Single-transfer task preparation
//! - [`BatchTaskPreparer`]: Optimized batch preparation (recommended for high throughput)
//!
//! ## BatchTaskPreparer Optimization
//!
//! | Metric | Before (per-tx) | After (batch) | Improvement |
//! |--------|-----------------|---------------|-------------|
//! | Lock acquisitions | 5-6N | 2 | ~99.6% |
//! | state_root calc | N | 1 | ~99.9% |
//!
//! ## Flow
//!
//! ```text
//! User Request (Transfer)
//!       │
//!       ▼
//! Validator.prepare_solver_task()
//!       │
//!       ├── 1. Convert Transfer to Event (account model)
//!       ├── 2. Select coins for sender (object model)
//!       ├── 3. Build ResolvedInputs with object references
//!       ├── 4. Build read_set with Merkle proofs
//!       ├── 5. Generate task_id for Attestation binding
//!       └── 6. Create SolverTask
//!       │
//!       ▼
//! SolverTask → Solver → TEE
//! ```
//!
//! ## StateProvider
//!
//! The `StateProvider` trait and `MerkleStateProvider` implementation are
//! defined in `setu_storage::state_provider`. Use `TaskPreparer::new_for_testing()`
//! for tests, which creates a real MerkleStateProvider with pre-initialized accounts.

mod single;
mod batch;

// Re-export main types
pub use single::TaskPreparer;
pub use batch::{BatchTaskPreparer, BatchPrepareResult, BatchPrepareStats};

// Re-export shared types from storage
pub use setu_storage::{StateProvider, CoinInfo, SimpleMerkleProof, BatchStateSnapshot, BatchSnapshotStats};

use setu_types::task::MerkleProof;

/// Errors during task preparation
#[derive(Debug, thiserror::Error, Clone)]
pub enum TaskPrepareError {
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: u64, available: u64 },
    
    #[error("No coins found for address {0}")]
    NoCoinsFound(String),
    
    #[error("Object not found: {0}")]
    ObjectNotFound(String),
    
    #[error("Failed to create event: {0}")]
    EventCreationFailed(String),
    
    #[error("Merkle proof not available for object {0}")]
    MerkleProofNotAvailable(String),
    
    #[error("All {coin_count} coins for sender {sender} are currently reserved")]
    AllCoinsReserved { sender: String, coin_count: usize },
}

/// Convert SimpleMerkleProof to MerkleProof (for TEE)
#[allow(dead_code)]
pub(crate) fn to_enclave_proof(proof: &SimpleMerkleProof) -> MerkleProof {
    MerkleProof {
        siblings: proof.siblings.clone(),
        path_bits: proof.path_bits.clone(),
        leaf_index: Some(0),
    }
}

// ============================================================================
// Shared Test Utilities
// ============================================================================

/// Create a MerkleStateProvider with pre-initialized test accounts.
///
/// This is a shared utility function to avoid code duplication between
/// `TaskPreparer::new_for_testing()` and `BatchTaskPreparer::new_for_testing()`.
///
/// ## Initialized accounts (20 total):
/// - `alice`, `bob`, `charlie`: 10,000,000 balance each
/// - `user_01` to `user_17`: 5,000,000 balance each
///
/// ## Usage
///
/// ```rust,ignore
/// let state_provider = create_test_state_provider();
/// let preparer = TaskPreparer::new("validator-1".to_string(), state_provider);
/// ```
pub fn create_test_state_provider() -> std::sync::Arc<setu_storage::MerkleStateProvider> {
    use setu_storage::{GlobalStateManager, MerkleStateProvider, init_coin};
    use std::sync::{Arc, RwLock};

    let state_manager = Arc::new(RwLock::new(GlobalStateManager::new()));

    // Initialize test accounts with real Merkle state
    // 20 accounts with sufficient balance for large-scale benchmarks
    // This reduces state lock contention compared to just 3 accounts
    {
        let mut manager = state_manager.write()
            .expect("Failed to acquire write lock on GlobalStateManager during test initialization");
        // Primary accounts with higher balance
        init_coin(&mut manager, "alice", 10_000_000);
        init_coin(&mut manager, "bob", 10_000_000);
        init_coin(&mut manager, "charlie", 10_000_000);
        // Additional test accounts (user_01 to user_17)
        for i in 1..=17 {
            init_coin(&mut manager, &format!("user_{:02}", i), 5_000_000);
        }
    }

    Arc::new(MerkleStateProvider::new(state_manager))
}
