# Setu Runtime

A simplified runtime execution environment for validating Setu's core mechanisms before introducing the Move VM.

## Features

- ✅ Object model-based state management
- ✅ Coin transfers (full and partial transfers)
- ✅ Balance queries
- ✅ Object ownership management
- ✅ State change tracking
- 🔄 Future TEE execution support
- 🔄 Future Move VM support

## Design Philosophy

### 1. Simple Move VM Alternative

The current implementation uses simple state transition functions to replace the Move VM, reducing the complexity of early development while preserving Setu's core mechanisms:

- **Object Model**: Similar to Sui's object system
- **Ownership Management**: Objects have clear owners
- **State Change Tracking**: Records all state changes

### 2. Modular Design

Runtime is an independent crate, decoupled from other components:

```
Validator -> Solver -> Runtime -> State Store
```

Can be smoothly replaced with Move VM in the future:

```rust
// Current: Using RuntimeExecutor
let executor = RuntimeExecutor::new(state_store);
let output = executor.execute_transaction(&tx, &ctx)?;

// Future: Replace with Move VM Executor
let executor = MoveVMExecutor::new(state_store);
let output = executor.execute_transaction(&tx, &ctx)?;
```

### 3. Transaction Types

Currently supports two transaction types:

#### Transfer

- **Full Transfer**: Direct ownership transfer, zero-copy
- **Partial Transfer**: Split coin, create new object for recipient

```rust
// Full transfer
let tx = Transaction::new_transfer(sender, coin_id, recipient, None);

// Partial transfer of 300
let tx = Transaction::new_transfer(sender, coin_id, recipient, Some(300));
```

#### Query

Read-only operations, no state modification:

- Query balance
- Query object
- Query owned objects

```rust
let tx = Transaction::new_balance_query(address);
```

## Usage Examples

### Basic Transfer Flow

```rust
use setu_runtime::{
    RuntimeExecutor, ExecutionContext,
    Transaction, InMemoryStateStore,
};
use setu_types::{Address, create_coin, CoinType};

// 1. Create state storage
let mut store = InMemoryStateStore::new();

// 2. Initialize some coins
let alice = Address::from_str("0xalice")?;
let coin = create_coin(alice.clone(), 1000, CoinType::native());
let coin_id = coin.id;
store.set_object(coin_id, coin)?;

// 3. Create executor
let mut executor = RuntimeExecutor::new(store);

// 4. Create transfer transaction
let bob = Address::from_str("0xbob")?;
let tx = Transaction::new_transfer(
    alice.clone(),  // sender
    coin_id,        // coin to transfer
    bob.clone(),    // recipient
    Some(300),      // amount (partial transfer)
);

// 5. Execute transaction
let ctx = ExecutionContext {
    executor_id: "solver1".to_string(),
    timestamp: 1000,
    in_tee: false,
};

let output = executor.execute_transaction(&tx, &ctx)?;

// 6. Check results
assert!(output.success);
println!("State changes: {}", output.state_changes.len());
println!("Created objects: {:?}", output.created_objects);
```

### Integration with Solver

Modify `setu-solver/src/executor.rs`:

```rust
use setu_runtime::{RuntimeExecutor, ExecutionContext, Transaction};

pub struct Executor {
    runtime: RuntimeExecutor<InMemoryStateStore>,
    node_id: String,
}

impl Executor {
    pub async fn execute_in_tee(&self, tx: &Transaction) -> anyhow::Result<ExecutionOutput> {
        let ctx = ExecutionContext {
            executor_id: self.node_id.clone(),
            timestamp: current_timestamp(),
            in_tee: false, // TODO: Execute in TEE in the future
        };
        
        self.runtime.execute_transaction(tx, &ctx)
    }
}
```

## State Change Tracking

Each transaction execution returns detailed state changes:

```rust
pub struct StateChange {
    pub change_type: StateChangeType, // Create/Update/Delete
    pub object_id: ObjectId,
    pub old_state: Option<Vec<u8>>,   // Serialized old state
    pub new_state: Option<Vec<u8>>,   // Serialized new state
}
```

These state changes are used by:

1. **Solver** to build Events
2. **Validator** to verify execution results
3. **DAG** to record dependency relationships

## Integration with Main Flow

```
External Transaction 
  -> Validator (routing)
  -> Solver (dependency resolution)
  -> Runtime.execute_transaction() (execution)
  -> ExecutionOutput (result + state changes + dependent objects)
  -> Event (build event)
  -> Validator DAG (verification + voting)
  -> Folding (Anchor)
```