#!/bin/bash
# Stop Setu Docker services

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"

cd "$DOCKER_DIR"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║           Stopping Setu Docker Services                    ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Parse command line arguments
REMOVE_VOLUMES=false
MODE="all"

while [[ $# -gt 0 ]]; do
    case $1 in
        -v|--volumes)
            REMOVE_VOLUMES=true
            shift
            ;;
        --multi-validator)
            MODE="multi-validator"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [-v|--volumes] [--multi-validator]"
            exit 1
            ;;
    esac
done

# Stop services
if [ "$MODE" = "multi-validator" ]; then
    echo "Stopping multi-validator setup..."
    if [ "$REMOVE_VOLUMES" = true ]; then
        docker-compose -f docker-compose.multi-validator.yml down -v
        echo "  └─ ✓ Services stopped and volumes removed"
    else
        docker-compose -f docker-compose.multi-validator.yml down
        echo "  └─ ✓ Services stopped (volumes preserved)"
    fi
else
    echo "Stopping services..."
    if [ "$REMOVE_VOLUMES" = true ]; then
        docker-compose -f docker-compose.yml down -v --remove-orphans
        echo "  └─ ✓ Services stopped and volumes removed"
    else
        docker-compose -f docker-compose.yml down --remove-orphans
        echo "  └─ ✓ Services stopped (volumes preserved)"
    fi
fi

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║           Services Stopped                                 ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

if [ "$REMOVE_VOLUMES" = false ]; then
    echo "Note: Data volumes are preserved."
    echo "To remove volumes, run: $0 --volumes"
    echo ""
fi

