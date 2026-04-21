#!/usr/bin/env bash
# Start Aggregated Plan — backend (port 3001) + frontend (port 3000)
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOG_DIR="$PROJECT_DIR/logs"
mkdir -p "$LOG_DIR"

# Source nvm so cargo/node/pnpm are on PATH
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && source "$NVM_DIR/nvm.sh"

# Backend
echo "[$(date)] Starting backend..." | tee -a "$LOG_DIR/backend.log"
cd "$PROJECT_DIR/backend"
cargo run -p api >> "$LOG_DIR/backend.log" 2>&1 &
BACKEND_PID=$!

# Frontend
echo "[$(date)] Starting frontend..." | tee -a "$LOG_DIR/frontend.log"
cd "$PROJECT_DIR/frontend"
pnpm dev >> "$LOG_DIR/frontend.log" 2>&1 &
FRONTEND_PID=$!

echo "[$(date)] Backend PID=$BACKEND_PID, Frontend PID=$FRONTEND_PID"

# Graceful shutdown on SIGTERM/SIGINT
cleanup() {
    echo "[$(date)] Stopping..."
    kill "$FRONTEND_PID" "$BACKEND_PID" 2>/dev/null || true
    wait "$FRONTEND_PID" "$BACKEND_PID" 2>/dev/null || true
    echo "[$(date)] Stopped."
}
trap cleanup SIGTERM SIGINT

# Wait for both
wait
