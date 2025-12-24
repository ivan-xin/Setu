#!/bin/bash

# Setu Node Individual Test Script
# Test validator and solver separately

set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=================================="
echo "Setu Node Individual Test"
echo "=================================="
echo ""

# Build first
echo -e "${BLUE}Building project...${NC}"
cargo build --bin setu-validator --bin setu-solver
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Menu
echo "Select node to test:"
echo "  1) Validator only"
echo "  2) Solver only"
echo "  3) Both (validator + solver)"
echo ""
read -p "Enter choice [1-3]: " choice

case $choice in
    1)
        echo ""
        echo -e "${BLUE}Starting Validator...${NC}"
        NODE_ID=validator1 PORT=8001 ./target/debug/setu-validator
        ;;
    2)
        echo ""
        echo -e "${BLUE}Starting Solver...${NC}"
        NODE_ID=solver1 PORT=9001 ./target/debug/setu-solver
        ;;
    3)
        echo ""
        echo -e "${BLUE}Starting both nodes...${NC}"
        ./scripts/test_nodes.sh
        ;;
    *)
        echo "Invalid choice"
        exit 1
        ;;
esac

