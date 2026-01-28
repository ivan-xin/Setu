#!/bin/bash
# Build Docker images for Setu

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_DIR="$(dirname "$DOCKER_DIR")"

cd "$PROJECT_DIR"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║           Building Setu Docker Images                      ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Enable BuildKit for faster builds
export DOCKER_BUILDKIT=1

# Build Validator image
echo "[1/2] Building Validator image..."
docker build \
    -f docker/Dockerfile.validator \
    -t setu-validator:latest \
    -t setu-validator:$(git rev-parse --short HEAD 2>/dev/null || echo "dev") \
    .

echo "  └─ ✓ Validator image built successfully"
echo ""

# Build Solver image
echo "[2/2] Building Solver image..."
docker build \
    -f docker/Dockerfile.solver \
    -t setu-solver:latest \
    -t setu-solver:$(git rev-parse --short HEAD 2>/dev/null || echo "dev") \
    .

echo "  └─ ✓ Solver image built successfully"
echo ""

echo "╔════════════════════════════════════════════════════════════╗"
echo "║           Build Complete                                   ║"
echo "╠════════════════════════════════════════════════════════════╣"
echo "║  Images:                                                   ║"
echo "║    - setu-validator:latest                                ║"
echo "║    - setu-solver:latest                                   ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Next steps:"
echo "  ./scripts/start.sh              # Start services"
echo "  docker images | grep setu       # List images"
echo ""

