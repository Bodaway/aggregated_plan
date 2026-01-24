#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BACKEND_INSPECT_HOST="${BACKEND_INSPECT_HOST:-127.0.0.1}"
BACKEND_INSPECT_PORT="${BACKEND_INSPECT_PORT:-9229}"
BACKEND_INSPECT_ENABLED="${BACKEND_INSPECT_ENABLED:-true}"
TMP_DIR="${TMP_DIR:-$ROOT_DIR/.tmp}"

mkdir -p "$TMP_DIR"

echo "Starting backend with inspector on ${BACKEND_INSPECT_HOST}:${BACKEND_INSPECT_PORT}"
(
  cd "$ROOT_DIR"
  if [ "$BACKEND_INSPECT_ENABLED" = "true" ]; then
    if ! TMPDIR="$TMP_DIR" pnpm --filter backend exec -- node --inspect="${BACKEND_INSPECT_HOST}:${BACKEND_INSPECT_PORT}" --import tsx src/index.ts; then
      echo "Inspector unavailable, restarting backend without inspector"
      TMPDIR="$TMP_DIR" pnpm --filter backend exec -- node --import tsx src/index.ts
    fi
  else
    TMPDIR="$TMP_DIR" pnpm --filter backend exec -- node --import tsx src/index.ts
  fi
) &
BACKEND_PID=$!

cleanup() {
  if kill -0 "$BACKEND_PID" >/dev/null 2>&1; then
    kill "$BACKEND_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

echo "Starting frontend dev server"
(
  cd "$ROOT_DIR"
  pnpm --filter frontend dev
)
