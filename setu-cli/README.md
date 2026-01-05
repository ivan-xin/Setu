# Setu CLI

Command-line interface for registering and managing Solver and Validator nodes in the Setu network.

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

# Register a validator
setu-cli validator register --id validator-1 --address 127.0.0.1:8001

# Register a solver
setu-cli solver register --id solver-1 --address 127.0.0.1:9001 --capacity 100

# List registered validators
setu-cli validator list

# List registered solvers
setu-cli solver list
```

## Commands

### Validator

Manage validator nodes.

```bash
# Register a validator
setu-cli validator register \
  --id <validator_id> \
  --address <address> \
  [--stake <stake_amount>]

# List all validators
setu-cli validator list

# Unregister a validator
setu-cli validator unregister --id <validator_id>

# Check validator status
setu-cli validator status --id <validator_id>
```

### Solver

Manage solver nodes.

```bash
# Register a solver
setu-cli solver register \
  --id <solver_id> \
  --address <address> \
  --capacity <capacity> \
  [--shard <shard_id>] \
  [--resources <resource1,resource2,...>]

# List all solvers
setu-cli solver list

# Unregister a solver
setu-cli solver unregister --id <solver_id>

# Check solver status
setu-cli solver status --id <solver_id>

# Update solver capacity
setu-cli solver update --id <solver_id> --capacity <new_capacity>
```

### Status

Check system status.

```bash
# Check overall system status
setu-cli status

# Check specific node
setu-cli status --node <node_id>
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

### Register Validators (3 nodes for BFT)

```bash
setu-cli validator register --id validator-1 --address 127.0.0.1:8001 --stake 1000
setu-cli validator register --id validator-2 --address 127.0.0.1:8002 --stake 1000
setu-cli validator register --id validator-3 --address 127.0.0.1:8003 --stake 1000
```

### Register Solvers (6 nodes)

```bash
setu-cli solver register --id solver-1 --address 127.0.0.1:9001 --capacity 100
setu-cli solver register --id solver-2 --address 127.0.0.1:9002 --capacity 100
setu-cli solver register --id solver-3 --address 127.0.0.1:9003 --capacity 100
setu-cli solver register --id solver-4 --address 127.0.0.1:9004 --capacity 100
setu-cli solver register --id solver-5 --address 127.0.0.1:9005 --capacity 100
setu-cli solver register --id solver-6 --address 127.0.0.1:9006 --capacity 100
```

### Check System Status

```bash
# List all nodes
setu-cli validator list
setu-cli solver list

# Check overall status
setu-cli status
```
