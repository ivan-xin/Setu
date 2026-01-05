# Setu CLI

Command-line interface for interacting with the Setu network.

## Installation

```bash
cargo install --path setu-cli
```

Or build from source:

```bash
cargo build -p setu-cli --release
```

## Usage

```bash
# Show help
setu-cli --help

# Submit a transfer
setu-cli transfer --from alice --to bob --amount 100

# Query account balance
setu-cli balance --account alice

# Check node status
setu-cli status --node validator-1

# List registered solvers
setu-cli solvers list

# Register a new solver
setu-cli solvers register --id solver-1 --address 127.0.0.1:9001
```

## Commands

### Transfer

Submit a transfer to the network.

```bash
setu-cli transfer \
  --from <sender> \
  --to <recipient> \
  --amount <amount> \
  [--solver <preferred_solver>]
```

### Balance

Query account balance.

```bash
setu-cli balance --account <account_id>
```

### Status

Check node status.

```bash
setu-cli status --node <node_id>
```

### Solvers

Manage solver nodes.

```bash
# List all solvers
setu-cli solvers list

# Register a solver
setu-cli solvers register --id <solver_id> --address <address>

# Unregister a solver
setu-cli solvers unregister --id <solver_id>
```

## Configuration

The CLI can be configured via:

1. Command-line arguments
2. Environment variables
3. Config file (`~/.setu/config.toml`)

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| SETU_RPC_URL | RPC endpoint URL | http://localhost:8000 |
| SETU_TIMEOUT | Request timeout (seconds) | 30 |

### Config File

```toml
# ~/.setu/config.toml
rpc_url = "http://localhost:8000"
timeout = 30
```

## Examples

### Submit a Transfer

```bash
# Simple transfer
setu-cli transfer --from alice --to bob --amount 100

# Transfer with preferred solver
setu-cli transfer --from alice --to bob --amount 100 --solver solver-1
```

### Check System Status

```bash
# Check validator status
setu-cli status --node validator-1

# Check all nodes
setu-cli status --all
```

