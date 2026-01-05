# Setu Solver

Solver node for the Setu network. Responsible for executing transfers and generating events.

## Responsibilities

- Receive transfer intents from Validators
- Execute computations (with optional TEE)
- Track dependencies between events
- Generate events with execution results
- Broadcast events to Validators

## Architecture

```
┌─────────────────────────────────────────┐
│                Solver                    │
├─────────────────────────────────────────┤
│  ┌─────────────────────────────────┐   │
│  │  Dependency Tracker              │   │
│  │  - Find parent events            │   │
│  │  - Build dependency graph        │   │
│  │  - Track resource conflicts      │   │
│  └─────────────────────────────────┘   │
│  ┌─────────────────────────────────┐   │
│  │  Executor                        │   │
│  │  - Execute transfers             │   │
│  │  - Apply state changes           │   │
│  │  - Generate execution result     │   │
│  └─────────────────────────────────┘   │
│  ┌─────────────────────────────────┐   │
│  │  TEE Environment                 │   │
│  │  - Secure execution              │   │
│  │  - Generate attestation          │   │
│  │  - Proof generation              │   │
│  └─────────────────────────────────┘   │
│  ┌─────────────────────────────────┐   │
│  │  VLC Manager                     │   │
│  │  - Update logical clock          │   │
│  │  - Create VLC snapshots          │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

## Execution Flow

```
1. Receive Transfer
       ↓
2. Find Dependencies (parent events)
       ↓
3. Execute in TEE (optional)
       ↓
4. Apply State Changes
       ↓
5. Generate TEE Proof
       ↓
6. Update VLC
       ↓
7. Create Event
       ↓
8. Send to Validator
```

## Usage

### As a Binary

```bash
# Start solver with default config
cargo run -p setu-solver

# With environment variables
SOLVER_ID=solver_1 \
SOLVER_PORT=9001 \
SOLVER_STAKE=1000 \
cargo run -p setu-solver
```

### As a Library

```rust
use setu_solver::Solver;
use setu_core::NodeConfig;
use tokio::sync::mpsc;

let config = NodeConfig {
    node_id: "solver-1".to_string(),
    // ...
};

let (transfer_tx, transfer_rx) = mpsc::unbounded_channel();
let (event_tx, event_rx) = mpsc::unbounded_channel();

let solver = Solver::new(config, transfer_rx, event_tx);

// Run the solver
solver.run().await;
```

## Configuration

| Parameter | Environment Variable | Default | Description |
|-----------|---------------------|---------|-------------|
| node_id | SOLVER_ID | auto-generated | Unique solver identifier |
| port | SOLVER_PORT | 9001 | Network port |
| stake | SOLVER_STAKE | 1000 | Solver stake amount |
| tee_enabled | TEE_ENABLED | false | Enable TEE execution |

## Modules

- `executor.rs` - Transfer execution logic
- `dependency.rs` - Dependency tracking
- `tee.rs` - TEE environment simulation

## Dependencies

- `setu-core` - Core configuration
- `setu-types` - Type definitions
- `setu-vlc` - VLC clock
- `core-types` - Core type definitions

## Notes

- Keep interfaces narrow; produce transfers that pass DAG/TEE verification
- Consider batching outputs to match fold windows
- Could house domain-specific logic (e.g., AI task results) whose proofs feed into transfers
