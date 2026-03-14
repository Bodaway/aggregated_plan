#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────
# deploy.sh — Deploy Aggregated Plan infrastructure to Azure
# Usage: ./infra/deploy.sh [dev|staging|prod]
# Prerequisites: az cli, logged in (az login)
# ─────────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

ENV="${1:-dev}"
PROJECT="aggplan"
LOCATION="${AZURE_LOCATION:-westeurope}"
RG_NAME="rg-${PROJECT}-${ENV}"
IMAGE_TAG="${IMAGE_TAG:-latest}"

echo "══════════════════════════════════════════════════════"
echo "  Aggregated Plan — Azure Deployment"
echo "  Environment: ${ENV}"
echo "  Resource Group: ${RG_NAME}"
echo "  Location: ${LOCATION}"
echo "══════════════════════════════════════════════════════"

# ── 1. Create resource group ──
echo ""
echo "→ Creating resource group..."
az group create \
  --name "$RG_NAME" \
  --location "$LOCATION" \
  --tags project=aggregated-plan environment="$ENV" managedBy=bicep \
  --output none

# ── 2. Deploy Bicep template ──
echo "→ Deploying infrastructure (Bicep)..."
DEPLOY_OUTPUT=$(az deployment group create \
  --resource-group "$RG_NAME" \
  --template-file "$SCRIPT_DIR/main.bicep" \
  --parameters "$SCRIPT_DIR/parameters/${ENV}.bicepparam" \
  --parameters backendImageTag="$IMAGE_TAG" \
  --query 'properties.outputs' \
  --output json)

ACR_LOGIN_SERVER=$(echo "$DEPLOY_OUTPUT" | jq -r '.acrLoginServer.value')
BACKEND_URL=$(echo "$DEPLOY_OUTPUT" | jq -r '.backendUrl.value')
FRONTEND_URL=$(echo "$DEPLOY_OUTPUT" | jq -r '.frontendUrl.value')
SWA_TOKEN=$(echo "$DEPLOY_OUTPUT" | jq -r '.swaDeploymentToken.value')

echo ""
echo "══════════════════════════════════════════════════════"
echo "  Infrastructure deployed!"
echo "──────────────────────────────────────────────────────"
echo "  ACR:      ${ACR_LOGIN_SERVER}"
echo "  Backend:  ${BACKEND_URL}"
echo "  Frontend: ${FRONTEND_URL}"
echo "══════════════════════════════════════════════════════"

# ── 3. Build & push backend image ──
echo ""
echo "→ Building and pushing backend image..."
"$SCRIPT_DIR/build-and-push.sh" "$ACR_LOGIN_SERVER" "$IMAGE_TAG"

# ── 4. Update container app with new image ──
echo "→ Updating backend container app..."
ACR_NAME=$(echo "$ACR_LOGIN_SERVER" | cut -d'.' -f1)
az containerapp update \
  --name "ca-backend-${PROJECT}${ENV}" \
  --resource-group "$RG_NAME" \
  --image "${ACR_LOGIN_SERVER}/aggregated-plan-backend:${IMAGE_TAG}" \
  --output none

# ── 5. Deploy frontend to Static Web App ──
echo "→ Building frontend..."
cd "$ROOT_DIR/frontend"

# Set backend URL for the build
export VITE_API_URL="$BACKEND_URL"
export VITE_GRAPHQL_URL="${BACKEND_URL}/graphql"

if [ -f "pnpm-lock.yaml" ]; then
  pnpm install --frozen-lockfile
  pnpm build
elif [ -f "package-lock.json" ]; then
  npm ci
  npm run build
fi

echo "→ Deploying frontend to Static Web App..."
npx --yes @azure/static-web-apps-cli deploy \
  ./dist \
  --deployment-token "$SWA_TOKEN" \
  --env production

echo ""
echo "══════════════════════════════════════════════════════"
echo "  Deployment complete!"
echo "  Backend:  ${BACKEND_URL}"
echo "  Frontend: ${FRONTEND_URL}"
echo "══════════════════════════════════════════════════════"
