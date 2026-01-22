#!/bin/bash
# View logs from Setu Docker services

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"

cd "$DOCKER_DIR"

# Parse command line arguments
SERVICE=""
FOLLOW="-f"
TAIL="100"
MODE="single"

while [[ $# -gt 0 ]]; do
    case $1 in
        validator|validator-1|validator-2|validator-3|solver-1|solver-2)
            SERVICE="$1"
            shift
            ;;
        --no-follow)
            FOLLOW=""
            shift
            ;;
        --tail)
            TAIL="$2"
            shift 2
            ;;
        --multi-validator)
            MODE="multi-validator"
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [SERVICE] [OPTIONS]"
            echo ""
            echo "Services:"
            echo "  validator, validator-1, validator-2, validator-3"
            echo "  solver-1, solver-2"
            echo ""
            echo "Options:"
            echo "  --no-follow          Don't follow log output"
            echo "  --tail N             Show last N lines (default: 100)"
            echo "  --multi-validator    Use multi-validator compose file"
            echo "  -h, --help           Show this help"
            echo ""
            echo "Examples:"
            echo "  $0                   # Show all logs"
            echo "  $0 validator         # Show validator logs"
            echo "  $0 solver-1 --tail 50  # Show last 50 lines of solver-1"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Run '$0 --help' for usage information"
            exit 1
            ;;
    esac
done

# Select compose file
if [ "$MODE" = "multi-validator" ]; then
    COMPOSE_FILE="docker-compose.multi-validator.yml"
else
    COMPOSE_FILE="docker-compose.yml"
fi

# Show logs
if [ -z "$SERVICE" ]; then
    echo "Showing logs from all services..."
    docker-compose -f "$COMPOSE_FILE" logs $FOLLOW --tail="$TAIL"
else
    echo "Showing logs from $SERVICE..."
    docker-compose -f "$COMPOSE_FILE" logs $FOLLOW --tail="$TAIL" "$SERVICE"
fi

