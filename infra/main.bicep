// ─────────────────────────────────────────────────────────────
// Aggregated Plan — Azure Infrastructure (minimum cost)
// ─────────────────────────────────────────────────────────────
// Resources:
//   - ACR Basic (~5€/month)
//   - Container Apps Consumption (scale-to-zero, ~0€ idle)
//   - Azure Static Web Apps Free (0€)
//   - Storage Account + File Share for SQLite (~0.01€/month)
// ─────────────────────────────────────────────────────────────

targetScope = 'resourceGroup'

@description('Environment name (dev, staging, prod)')
@allowed(['dev', 'staging', 'prod'])
param environment string = 'dev'

@description('Azure region')
param location string = resourceGroup().location

@description('Project base name')
param projectName string = 'aggplan'

@description('Backend container image tag')
param backendImageTag string = 'latest'

// ── Naming convention ──
var suffix = '${projectName}${environment}'
var acrName = replace('acr${suffix}', '-', '')
var storageAccountName = replace('st${suffix}', '-', '')
var envName = 'cae-${suffix}'
var backendAppName = 'ca-backend-${suffix}'
var swaName = 'swa-${suffix}'
var tags = {
  project: 'aggregated-plan'
  environment: environment
  managedBy: 'bicep'
}

// ── Container Registry ──
module acr 'modules/container-registry.bicep' = {
  name: 'deploy-acr'
  params: {
    name: acrName
    location: location
    sku: 'Basic'
    tags: tags
  }
}

// ── Storage for SQLite ──
module storage 'modules/storage.bicep' = {
  name: 'deploy-storage'
  params: {
    name: storageAccountName
    location: location
    tags: tags
  }
}

// ── Container Apps (Backend) ──
module containerApps 'modules/container-apps.bicep' = {
  name: 'deploy-container-apps'
  params: {
    environmentName: envName
    backendAppName: backendAppName
    location: location
    acrLoginServer: acr.outputs.loginServer
    acrUsername: acrName
    acrPassword: listCredentials(acr.outputs.id, '2023-07-01').passwords[0].value
    backendImage: '${acr.outputs.loginServer}/aggregated-plan-backend:${backendImageTag}'
    storageAccountName: storage.outputs.storageAccountName
    storageAccountKey: storage.outputs.storageAccountKey
    fileShareName: storage.outputs.fileShareName
    tags: tags
  }
}

// ── Static Web App (Frontend) ──
module swa 'modules/static-web-app.bicep' = {
  name: 'deploy-swa'
  params: {
    name: swaName
    location: location
    backendUrl: containerApps.outputs.backendUrl
    tags: tags
  }
}

// ── Outputs ──
output acrLoginServer string = acr.outputs.loginServer
output backendUrl string = containerApps.outputs.backendUrl
output frontendUrl string = swa.outputs.url
output swaDeploymentToken string = swa.outputs.deploymentToken
