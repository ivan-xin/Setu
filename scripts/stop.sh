#!/bin/bash
# Setu Stop Script
# 
# This script stops all running Setu nodes

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo "Stopping Setu Network..."
echo ""

# Stop from PID files
if [ -d "pids" ]; then
    for pid_file in pids/*.pid; do
        if [ -f "$pid_file" ]; then
            pid=$(cat "$pid_file")
            if ps -p $pid > /dev/null 2>&1; then
                echo "Stopping $(basename $pid_file .pid) (PID: $pid)..."
                kill $pid
            fi
            rm "$pid_file"
        fi
    done
fi

# Fallback: kill by process name
pkill -f "setu-validator" 2>/dev/null || true
pkill -f "setu-solver" 2>/dev/null || true

echo ""
echo "✓ All nodes stopped"

