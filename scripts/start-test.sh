#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
ENV_FILE="$ROOT_DIR/config/env/test.env"

if [ ! -f "$ENV_FILE" ]; then
  echo "Missing env file: $ENV_FILE" >&2
  exit 1
fi

cd "$ROOT_DIR"

if command -v docker >/dev/null 2>&1; then
  COMPOSE_BIN="docker"
elif command -v podman >/dev/null 2>&1; then
  COMPOSE_BIN="podman"
else
  echo "Missing docker or podman in PATH." >&2
  exit 1
fi

"$COMPOSE_BIN" compose --env-file "$ENV_FILE" -f infra/compose/compose.yaml up --build
