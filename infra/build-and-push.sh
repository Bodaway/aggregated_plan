#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────
# build-and-push.sh — Build and push Docker image to ACR
# Usage: ./infra/build-and-push.sh <acr-login-server> [tag]
# ─────────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

ACR_LOGIN_SERVER="${1:?Usage: $0 <acr-login-server> [tag]}"
TAG="${2:-latest}"

IMAGE_NAME="aggregated-plan-backend"
FULL_IMAGE="${ACR_LOGIN_SERVER}/${IMAGE_NAME}:${TAG}"

echo "→ Logging in to ACR: ${ACR_LOGIN_SERVER}..."
az acr login --name "$(echo "$ACR_LOGIN_SERVER" | cut -d'.' -f1)"

echo "→ Building image: ${FULL_IMAGE}..."
docker build \
  -f "$SCRIPT_DIR/Dockerfile.backend" \
  -t "$FULL_IMAGE" \
  "$ROOT_DIR"

echo "→ Pushing image: ${FULL_IMAGE}..."
docker push "$FULL_IMAGE"

echo "→ Done! Image pushed: ${FULL_IMAGE}"
