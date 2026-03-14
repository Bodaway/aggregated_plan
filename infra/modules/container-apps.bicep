@description('Name of the Container Apps Environment')
param environmentName string

@description('Name of the backend Container App')
param backendAppName string

@description('Location for the resources')
param location string

@description('ACR login server')
param acrLoginServer string

@description('ACR admin username')
param acrUsername string

@secure()
@description('ACR admin password')
param acrPassword string

@description('Backend container image (full path)')
param backendImage string

@description('Storage account name for SQLite volume')
param storageAccountName string

@secure()
@description('Storage account key')
param storageAccountKey string

@description('File share name')
param fileShareName string

param tags object = {}

// ── Container Apps Environment (Consumption = free when idle) ──
resource environment 'Microsoft.App/managedEnvironments@2023-05-01' = {
  name: environmentName
  location: location
  tags: tags
  properties: {
    workloadProfiles: [
      {
        name: 'Consumption'
        workloadProfileType: 'Consumption'
      }
    ]
  }
}

// ── Mount Azure File Share for SQLite persistence ──
resource envStorage 'Microsoft.App/managedEnvironments/storages@2023-05-01' = {
  parent: environment
  name: 'sqlitestore'
  properties: {
    azureFile: {
      accountName: storageAccountName
      accountKey: storageAccountKey
      shareName: fileShareName
      accessMode: 'ReadWrite'
    }
  }
}

// ── Backend Container App ──
resource backendApp 'Microsoft.App/containerApps@2023-05-01' = {
  name: backendAppName
  location: location
  tags: tags
  properties: {
    managedEnvironmentId: environment.id
    workloadProfileName: 'Consumption'
    configuration: {
      ingress: {
        external: true
        targetPort: 3001
        transport: 'http'
        allowInsecure: false
      }
      registries: [
        {
          server: acrLoginServer
          username: acrUsername
          passwordSecretRef: 'acr-password'
        }
      ]
      secrets: [
        {
          name: 'acr-password'
          value: acrPassword
        }
      ]
    }
    template: {
      containers: [
        {
          name: 'backend'
          image: backendImage
          resources: {
            cpu: json('0.25')
            memory: '0.5Gi'
          }
          env: [
            {
              name: 'DATABASE_URL'
              value: 'sqlite:/data/aggregated_plan.db?mode=rwc'
            }
            {
              name: 'RUST_LOG'
              value: 'info'
            }
          ]
          volumeMounts: [
            {
              volumeName: 'sqlite-volume'
              mountPath: '/data'
            }
          ]
        }
      ]
      volumes: [
        {
          name: 'sqlite-volume'
          storageType: 'AzureFile'
          storageName: 'sqlitestore'
        }
      ]
      scale: {
        minReplicas: 0
        maxReplicas: 1 // Single instance for SQLite (no concurrent writes)
      }
    }
  }
  dependsOn: [
    envStorage
  ]
}

@description('Backend app FQDN')
output backendFqdn string = backendApp.properties.configuration.ingress.fqdn

@description('Backend app URL')
output backendUrl string = 'https://${backendApp.properties.configuration.ingress.fqdn}'
