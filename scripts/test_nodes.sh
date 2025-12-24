#!/bin/bash

# Setu Node Test Script
# This script builds and runs validator and solver nodes for testing

set -e  # Exit on error

echo "=================================="
echo "Setu Node Test Script"
echo "=================================="
echo ""

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Step 1: Build the project
echo -e "${BLUE}Step 1: Building project...${NC}"
cargo build --bin setu-validator --bin setu-solver

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Build successful${NC}"
    echo ""
else
    echo -e "${RED}✗ Build failed${NC}"
    exit 1
fi

# Step 2: Start Validator
echo -e "${BLUE}Step 2: Starting Validator node...${NC}"
echo -e "${YELLOW}Configuration:${NC}"
echo "  NODE_ID=validator1"
echo "  PORT=8001"
echo ""

NODE_ID=validator1 \
PORT=8001 \
./target/debug/setu-validator &

VALIDATOR_PID=$!
echo -e "${GREEN}✓ Validator started (PID: $VALIDATOR_PID)${NC}"
echo ""

# Wait a bit for validator to start
sleep 2

# Step 3: Start Solver
echo -e "${BLUE}Step 3: Starting Solver node...${NC}"
echo -e "${YELLOW}Configuration:${NC}"
echo "  NODE_ID=solver1"
echo "  PORT=9001"
echo ""

NODE_ID=solver1 \
PORT=9001 \
./target/debug/setu-solver &

SOLVER_PID=$!
echo -e "${GREEN}✓ Solver started (PID: $SOLVER_PID)${NC}"
echo ""

# Step 4: Monitor nodes
echo -e "${BLUE}Step 4: Monitoring nodes...${NC}"
echo "Press Ctrl+C to stop all nodes"
echo ""
echo "Running processes:"
echo "  Validator PID: $VALIDATOR_PID"
echo "  Solver PID: $SOLVER_PID"
echo ""

# Function to cleanup on exit
cleanup() {
    echo ""
    echo -e "${YELLOW}Stopping nodes...${NC}"
    kill $VALIDATOR_PID 2>/dev/null || true
    kill $SOLVER_PID 2>/dev/null || true
    echo -e "${GREEN}✓ All nodes stopped${NC}"
    exit 0
}

# Trap Ctrl+C
trap cleanup INT TERM

# Wait for processes
wait

