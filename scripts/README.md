# Setu Test Scripts

Test scripts for quickly starting and testing Setu nodes.

## Script Overview

### 1. test_nodes.sh - Start Multiple Nodes Together

Automatically builds and starts both validator and solver nodes.

**Usage:**
```bash
./scripts/test_nodes.sh
```

**Features:**
- ✅ Auto-compile project
- ✅ Start Validator (port 8001)
- ✅ Start Solver (port 9001)
- ✅ Display process PIDs
- ✅ Auto-cleanup all processes on Ctrl+C

**Output Example:**
```
==================================
Setu Node Test Script
==================================

Step 1: Building project...
✓ Build successful

Step 2: Starting Validator node...
Configuration:
  NODE_ID=validator1
  PORT=8001

✓ Validator started (PID: 12345)

Step 3: Starting Solver node...
Configuration:
  NODE_ID=solver1
  PORT=9001

✓ Solver started (PID: 12346)

Step 4: Monitoring nodes...
Press Ctrl+C to stop all nodes
```

### 2. test_single.sh - Test Individual Nodes

Interactive menu to choose which node(s) to start.

**Usage:**
```bash
./scripts/test_single.sh
```

**Features:**
- ✅ Interactive menu
- ✅ Option to start Validator only
- ✅ Option to start Solver only
- ✅ Option to start both

**Menu Example:**
```
Select node to test:
  1) Validator only
  2) Solver only
  3) Both (validator + solver)

Enter choice [1-3]:
```

## Manual Node Startup

If you want to start nodes manually:

### Start Validator
```bash
# Build first
cargo build --bin setu-validator

# Run
NODE_ID=validator1 PORT=8001 ./target/debug/setu-validator
```

### Start Solver
```bash
# Build first
cargo build --bin setu-solver

# Run
NODE_ID=solver1 PORT=9001 ./target/debug/setu-solver
```

## Environment Variables

Configure nodes via environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| NODE_ID | Node identifier | Random UUID |
| PORT | Listen port | 8000 |
| PEERS | Peer addresses | Empty |

**Example:**
```bash
NODE_ID=my_validator \
PORT=8001 \
PEERS=localhost:8002,localhost:8003 \
./target/debug/setu-validator
```

## Multi-Node Testing

Start 3 Validators and 2 Solvers:

```bash
# Terminal 1: Validator 1
NODE_ID=validator1 PORT=8001 ./target/debug/setu-validator

# Terminal 2: Validator 2
NODE_ID=validator2 PORT=8002 PEERS=localhost:8001 ./target/debug/setu-validator

# Terminal 3: Validator 3
NODE_ID=validator3 PORT=8003 PEERS=localhost:8001,localhost:8002 ./target/debug/setu-validator

# Terminal 4: Solver 1
NODE_ID=solver1 PORT=9001 ./target/debug/setu-solver

# Terminal 5: Solver 2
NODE_ID=solver2 PORT=9002 ./target/debug/setu-solver
```

## Viewing Logs

Nodes use `tracing` for logging, default level is INFO.

**Change log level:**
```bash
RUST_LOG=debug NODE_ID=validator1 ./target/debug/setu-validator
```

**Log levels:**
- `error`: Errors only
- `warn`: Warnings and errors
- `info`: Info, warnings and errors (default)
- `debug`: Debug information
- `trace`: Most detailed tracing

## Troubleshooting

### Port Already in Use
```bash
# Check port usage
lsof -i :8001

# Kill process using the port
kill -9 <PID>
```

### Build Failed
```bash
# Clean and rebuild
cargo clean
cargo build
```

### Node Won't Start
Check:
1. Is the port already in use?
2. Is the configuration correct?
3. Are all dependencies installed?

## Next Steps

- [ ] Add inter-node communication tests
- [ ] Add Transfer execution tests
- [ ] Add Shard routing tests
- [ ] Add performance tests

