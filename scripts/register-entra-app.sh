#!/usr/bin/env bash
#
# Registers an Entra ID (Azure AD) app for Aggregated Plan.
# Requires: Azure CLI (az) logged in with sufficient permissions.
#
# Usage: ./scripts/register-entra-app.sh
#
set -euo pipefail

APP_NAME="Aggregated Plan"
REDIRECT_URI="https://aggregatedplan-mbt-5cbd8789.eu.ngrok.io"
GRAPH_PERMISSION_FILES_READ="01d4889c-1f55-4f51-8b09-f7302e085bee"  # Files.Read.All delegated
GRAPH_PERMISSION_USER_READ="e1fe6dd8-ba31-4d61-89e7-88639da4683d"   # User.Read delegated
GRAPH_API_ID="00000003-0000-0000-c000-000000000000"  # Microsoft Graph

echo "Creating Entra ID app registration: ${APP_NAME}"

# Create the app registration
APP_ID=$(az ad app create \
  --display-name "${APP_NAME}" \
  --sign-in-audience "AzureADMyOrg" \
  --query "appId" \
  --output tsv)

echo "App registered with Client ID: ${APP_ID}"

# Set SPA redirect URI
az ad app update \
  --id "${APP_ID}" \
  --spa-redirect-uris "${REDIRECT_URI}"

# Add delegated permissions (User.Read + Files.Read.All)
az ad app permission add \
  --id "${APP_ID}" \
  --api "${GRAPH_API_ID}" \
  --api-permissions "${GRAPH_PERMISSION_USER_READ}=Scope" "${GRAPH_PERMISSION_FILES_READ}=Scope"

# Expose an API scope
SCOPE_ID=$(uuidgen || python3 -c "import uuid; print(uuid.uuid4())")
az ad app update \
  --id "${APP_ID}" \
  --identifier-uris "api://${APP_ID}" \
  --set "api={\"oauth2PermissionScopes\":[{\"id\":\"${SCOPE_ID}\",\"adminConsentDisplayName\":\"Access Aggregated Plan API\",\"adminConsentDescription\":\"Allow the app to access Aggregated Plan API on behalf of the signed-in user.\",\"userConsentDisplayName\":\"Access Aggregated Plan API\",\"userConsentDescription\":\"Allow the app to access Aggregated Plan API on your behalf.\",\"isEnabled\":true,\"type\":\"User\",\"value\":\"access_as_user\"}]}"

# Create a client secret (valid for 1 year)
SECRET=$(az ad app credential reset \
  --id "${APP_ID}" \
  --display-name "aggregated-plan-secret" \
  --years 1 \
  --query "password" \
  --output tsv)

# Get tenant ID
TENANT_ID=$(az account show --query "tenantId" --output tsv)

echo ""
echo "=== Configuration ==="
echo "Add these to your .env file:"
echo ""
echo "AZURE_AD_TENANT_ID=${TENANT_ID}"
echo "AZURE_AD_CLIENT_ID=${APP_ID}"
echo "AZURE_AD_CLIENT_SECRET=${SECRET}"
echo "AZURE_AD_SCOPE=api://${APP_ID}/access_as_user"
echo ""
echo "# Frontend (add to .env or vite environment)"
echo "VITE_AZURE_AD_CLIENT_ID=${APP_ID}"
echo "VITE_AZURE_AD_TENANT_ID=${TENANT_ID}"
echo "VITE_AZURE_AD_REDIRECT_URI=${REDIRECT_URI}"
echo "VITE_AZURE_AD_API_SCOPE=api://${APP_ID}/access_as_user"
echo ""
echo "Done. Remember to grant admin consent for the API permissions in the Azure portal."
