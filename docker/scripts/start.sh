#!/bin/bash
# Start Setu Docker services

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"

cd "$DOCKER_DIR"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║           Starting Setu Docker Services                    ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Check if .env exists, if not copy from example
if [ ! -f .env ]; then
    if [ -f .env.example ]; then
        echo "Creating .env from .env.example..."
        cp .env.example .env
        echo "  └─ ✓ .env created"
    fi
fi

# Parse command line arguments
MODE="single"
PROFILE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --multi-solver)
            PROFILE="--profile multi-solver"
            MODE="multi-solver"
            shift
            ;;
        --multi-validator)
            MODE="multi-validator"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--multi-solver|--multi-validator]"
            exit 1
            ;;
    esac
done

# Start services based on mode
if [ "$MODE" = "multi-validator" ]; then
    echo "Starting multi-validator setup (3 validators + 1 solver)..."
    docker-compose -f docker-compose.multi-validator.yml up -d
    
    echo ""
    echo "Waiting for services to start..."
    sleep 10
    
    echo ""
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║           Multi-Validator Network Started                  ║"
    echo "╠════════════════════════════════════════════════════════════╣"
    echo "║  Validator-1 API: http://localhost:8080/api/v1            ║"
    echo "║  Validator-2 API: http://localhost:8082/api/v1            ║"
    echo "║  Validator-3 API: http://localhost:8084/api/v1            ║"
    echo "║  Solver-1:        localhost:9001                          ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    
elif [ "$MODE" = "multi-solver" ]; then
    echo "Starting multi-solver setup (1 validator + 2 solvers)..."
    docker-compose -f docker-compose.yml $PROFILE up -d
    
    echo ""
    echo "Waiting for services to start..."
    sleep 10
    
    echo ""
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║           Multi-Solver Network Started                     ║"
    echo "╠════════════════════════════════════════════════════════════╣"
    echo "║  Validator API: http://localhost:8080/api/v1              ║"
    echo "║  Solver-1:      localhost:9001                            ║"
    echo "║  Solver-2:      localhost:9002                            ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    
else
    echo "Starting single-node setup (1 validator + 1 solver)..."
    docker-compose -f docker-compose.yml up -d
    
    echo ""
    echo "Waiting for services to start..."
    sleep 10
    
    echo ""
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║           Setu Network Started                             ║"
    echo "╠════════════════════════════════════════════════════════════╣"
    echo "║  Validator API: http://localhost:8080/api/v1              ║"
    echo "║  Solver-1:      localhost:9001                            ║"
    echo "╚════════════════════════════════════════════════════════════╝"
fi

echo ""
echo "Useful commands:"
echo "  docker-compose ps                    # Check status"
echo "  ./scripts/logs.sh                    # View logs"
echo "  ./scripts/stop.sh                    # Stop services"
echo "  curl http://localhost:8080/api/v1/health  # Health check"
echo ""

