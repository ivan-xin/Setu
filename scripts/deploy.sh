#!/bin/bash
# Setu Network Deployment Script
# 
# This script deploys a complete Setu network with:
# - 1 or 3 Validator nodes (configurable)
# - N Solver nodes (configurable)
# - Automatic registration and health checks

set -e

# ============================================
# Configuration
# ============================================

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
NUM_VALIDATORS=${NUM_VALIDATORS:-1}
NUM_SOLVERS=${NUM_SOLVERS:-3}
BASE_VALIDATOR_PORT=${BASE_VALIDATOR_PORT:-8080}
BASE_SOLVER_PORT=${BASE_SOLVER_PORT:-9001}
LISTEN_ADDR=${LISTEN_ADDR:-127.0.0.1}
WORKSPACE_DIR=${WORKSPACE_DIR:-$(pwd)}
LOG_DIR=${LOG_DIR:-"$WORKSPACE_DIR/logs"}
PID_DIR=${PID_DIR:-"$WORKSPACE_DIR/pids"}

# ============================================
# Helper Functions
# ============================================

print_header() {
    echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║${NC}  $1"
    echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
}

print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_step() {
    echo -e "${BLUE}[STEP $1/$2]${NC} $3"
}

check_command() {
    if ! command -v $1 &> /dev/null; then
        print_error "$1 is not installed. Please install it first."
        exit 1
    fi
}

wait_for_port() {
    local host=$1
    local port=$2
    local timeout=${3:-30}
    local elapsed=0
    
    while ! nc -z $host $port 2>/dev/null; do
        if [ $elapsed -ge $timeout ]; then
            return 1
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    return 0
}

check_health() {
    local url=$1
    local response=$(curl -s -o /dev/null -w "%{http_code}" $url 2>/dev/null || echo "000")
    if [ "$response" = "200" ]; then
        return 0
    else
        return 1
    fi
}

# ============================================
# Pre-flight Checks
# ============================================

preflight_checks() {
    print_header "Pre-flight Checks"
    
    print_step 1 4 "Checking required commands..."
    check_command cargo
    check_command nc
    check_command curl
    print_info "✓ All required commands are available"
    
    print_step 2 4 "Checking workspace..."
    if [ ! -f "$WORKSPACE_DIR/Cargo.toml" ]; then
        print_error "Not in Setu workspace directory"
        exit 1
    fi
    print_info "✓ Workspace directory: $WORKSPACE_DIR"
    
    print_step 3 4 "Creating directories..."
    mkdir -p "$LOG_DIR"
    mkdir -p "$PID_DIR"
    print_info "✓ Log directory: $LOG_DIR"
    print_info "✓ PID directory: $PID_DIR"
    
    print_step 4 4 "Building project..."
    cargo build --release
    print_info "✓ Build completed"
    
    echo ""
}

# ============================================
# Validator Deployment
# ============================================

deploy_validator() {
    local validator_id=$1
    local port=$2
    local is_leader=${3:-false}
    
    print_info "Deploying Validator: $validator_id on port $port"
    
    # Set environment variables
    export VALIDATOR_ID=$validator_id
    export VALIDATOR_HTTP_PORT=$port
    export VALIDATOR_LISTEN_ADDR=$LISTEN_ADDR
    export IS_LEADER=$is_leader
    
    # Start validator in background
    nohup cargo run --release -p setu-validator \
        > "$LOG_DIR/validator-$validator_id.log" 2>&1 &
    
    local pid=$!
    echo $pid > "$PID_DIR/validator-$validator_id.pid"
    
    print_info "  └─ PID: $pid"
    print_info "  └─ Log: $LOG_DIR/validator-$validator_id.log"
    
    # Wait for validator to start
    print_info "  └─ Waiting for validator to start..."
    if wait_for_port $LISTEN_ADDR $port 30; then
        print_info "  └─ ✓ Validator started successfully"
        
        # Check health
        sleep 2
        if check_health "http://$LISTEN_ADDR:$port/api/v1/health"; then
            print_info "  └─ ✓ Health check passed"
        else
            print_warn "  └─ ⚠ Health check failed (may still be initializing)"
        fi
    else
        print_error "  └─ ✗ Failed to start validator (timeout)"
        return 1
    fi
    
    echo ""
}

deploy_validators() {
    print_header "Deploying Validators"
    
    for i in $(seq 1 $NUM_VALIDATORS); do
        local validator_id="validator-$i"
        local port=$((BASE_VALIDATOR_PORT + i - 1))
        local is_leader="false"
        
        if [ $i -eq 1 ]; then
            is_leader="true"
        fi
        
        deploy_validator $validator_id $port $is_leader
    done
}

# ============================================
# Solver Deployment
# ============================================

deploy_solver() {
    local solver_id=$1
    local port=$2
    local validator_port=$3
    
    print_info "Deploying Solver: $solver_id on port $port"
    
    # Set environment variables
    export SOLVER_ID=$solver_id
    export SOLVER_PORT=$port
    export SOLVER_LISTEN_ADDR=$LISTEN_ADDR
    export SOLVER_CAPACITY=100
    export VALIDATOR_ADDRESS=$LISTEN_ADDR
    export VALIDATOR_HTTP_PORT=$validator_port
    export AUTO_REGISTER=true
    export HEARTBEAT_INTERVAL=30
    
    # Start solver in background
    nohup cargo run --release -p setu-solver \
        > "$LOG_DIR/solver-$solver_id.log" 2>&1 &
    
    local pid=$!
    echo $pid > "$PID_DIR/solver-$solver_id.pid"
    
    print_info "  └─ PID: $pid"
    print_info "  └─ Log: $LOG_DIR/solver-$solver_id.log"
    print_info "  └─ Validator: $LISTEN_ADDR:$validator_port"
    
    # Wait for solver to register
    print_info "  └─ Waiting for solver to register..."
    sleep 3
    
    # Check if solver is registered
    local response=$(curl -s "http://$LISTEN_ADDR:$validator_port/api/v1/solvers" 2>/dev/null || echo "{}")
    if echo "$response" | grep -q "$solver_id"; then
        print_info "  └─ ✓ Solver registered successfully"
    else
        print_warn "  └─ ⚠ Solver registration status unknown"
    fi
    
    echo ""
}

deploy_solvers() {
    print_header "Deploying Solvers"
    
    # Use first validator for solver registration
    local validator_port=$BASE_VALIDATOR_PORT
    
    for i in $(seq 1 $NUM_SOLVERS); do
        local solver_id="solver-$i"
        local port=$((BASE_SOLVER_PORT + i - 1))
        
        deploy_solver $solver_id $port $validator_port
    done
}

# ============================================
# Status Check
# ============================================

check_status() {
    print_header "Network Status"
    
    print_info "Validators:"
    for i in $(seq 1 $NUM_VALIDATORS); do
        local validator_id="validator-$i"
        local port=$((BASE_VALIDATOR_PORT + i - 1))
        local pid_file="$PID_DIR/validator-$validator_id.pid"
        
        if [ -f "$pid_file" ]; then
            local pid=$(cat "$pid_file")
            if ps -p $pid > /dev/null 2>&1; then
                if check_health "http://$LISTEN_ADDR:$port/api/v1/health"; then
                    print_info "  ✓ $validator_id (PID: $pid, Port: $port) - HEALTHY"
                else
                    print_warn "  ⚠ $validator_id (PID: $pid, Port: $port) - UNHEALTHY"
                fi
            else
                print_error "  ✗ $validator_id - NOT RUNNING"
            fi
        else
            print_error "  ✗ $validator_id - NOT DEPLOYED"
        fi
    done
    
    echo ""
    print_info "Solvers:"
    for i in $(seq 1 $NUM_SOLVERS); do
        local solver_id="solver-$i"
        local pid_file="$PID_DIR/solver-$solver_id.pid"
        
        if [ -f "$pid_file" ]; then
            local pid=$(cat "$pid_file")
            if ps -p $pid > /dev/null 2>&1; then
                print_info "  ✓ $solver_id (PID: $pid) - RUNNING"
            else
                print_error "  ✗ $solver_id - NOT RUNNING"
            fi
        else
            print_error "  ✗ $solver_id - NOT DEPLOYED"
        fi
    done
    
    echo ""
    
    # Query validator for registered solvers
    local validator_port=$BASE_VALIDATOR_PORT
    print_info "Registered Solvers (from Validator):"
    local response=$(curl -s "http://$LISTEN_ADDR:$validator_port/api/v1/solvers" 2>/dev/null || echo '{"solvers":[]}')
    local solver_count=$(echo "$response" | grep -o '"solver_id"' | wc -l)
    print_info "  Total: $solver_count solvers"
    
    echo ""
}

# ============================================
# Cleanup
# ============================================

cleanup() {
    print_header "Cleaning Up"
    
    print_info "Stopping all validators..."
    for pid_file in "$PID_DIR"/validator-*.pid; do
        if [ -f "$pid_file" ]; then
            local pid=$(cat "$pid_file")
            if ps -p $pid > /dev/null 2>&1; then
                kill $pid
                print_info "  └─ Stopped PID: $pid"
            fi
            rm "$pid_file"
        fi
    done
    
    print_info "Stopping all solvers..."
    for pid_file in "$PID_DIR"/solver-*.pid; do
        if [ -f "$pid_file" ]; then
            local pid=$(cat "$pid_file")
            if ps -p $pid > /dev/null 2>&1; then
                kill $pid
                print_info "  └─ Stopped PID: $pid"
            fi
            rm "$pid_file"
        fi
    done
    
    print_info "✓ Cleanup completed"
    echo ""
}

# ============================================
# Main Deployment Flow
# ============================================

deploy() {
    print_header "Setu Network Deployment"
    echo ""
    print_info "Configuration:"
    print_info "  Validators: $NUM_VALIDATORS"
    print_info "  Solvers: $NUM_SOLVERS"
    print_info "  Listen Address: $LISTEN_ADDR"
    print_info "  Base Validator Port: $BASE_VALIDATOR_PORT"
    print_info "  Base Solver Port: $BASE_SOLVER_PORT"
    echo ""
    
    # Pre-flight checks
    preflight_checks
    
    # Deploy validators
    deploy_validators
    
    # Wait a bit for validators to stabilize
    print_info "Waiting for validators to stabilize..."
    sleep 5
    echo ""
    
    # Deploy solvers
    deploy_solvers
    
    # Wait for solvers to register
    print_info "Waiting for solvers to complete registration..."
    sleep 5
    echo ""
    
    # Check status
    check_status
    
    # Print summary
    print_header "Deployment Complete"
    print_info "Validator API: http://$LISTEN_ADDR:$BASE_VALIDATOR_PORT/api/v1"
    print_info "Health Check: http://$LISTEN_ADDR:$BASE_VALIDATOR_PORT/api/v1/health"
    print_info "Logs: $LOG_DIR"
    print_info "PIDs: $PID_DIR"
    echo ""
    print_info "To stop the network, run: $0 stop"
    print_info "To check status, run: $0 status"
    echo ""
}

# ============================================
# Command Line Interface
# ============================================

show_usage() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  deploy    Deploy the Setu network (default)"
    echo "  stop      Stop all nodes"
    echo "  status    Check network status"
    echo "  clean     Stop nodes and clean up logs"
    echo "  help      Show this help message"
    echo ""
    echo "Environment Variables:"
    echo "  NUM_VALIDATORS       Number of validators (default: 1)"
    echo "  NUM_SOLVERS          Number of solvers (default: 3)"
    echo "  BASE_VALIDATOR_PORT  Base port for validators (default: 8080)"
    echo "  BASE_SOLVER_PORT     Base port for solvers (default: 9001)"
    echo "  LISTEN_ADDR          Listen address (default: 127.0.0.1)"
    echo ""
    echo "Examples:"
    echo "  $0 deploy"
    echo "  NUM_VALIDATORS=3 NUM_SOLVERS=10 $0 deploy"
    echo "  $0 status"
    echo "  $0 stop"
}

# Main entry point
case "${1:-deploy}" in
    deploy)
        deploy
        ;;
    stop)
        cleanup
        ;;
    status)
        check_status
        ;;
    clean)
        cleanup
        rm -rf "$LOG_DIR"/*
        print_info "Logs cleaned"
        ;;
    help|--help|-h)
        show_usage
        ;;
    *)
        print_error "Unknown command: $1"
        show_usage
        exit 1
        ;;
esac

