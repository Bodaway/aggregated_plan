@description('Name of the Static Web App')
param name string

@description('Location for the resource')
param location string

@description('Backend API URL for CORS/proxy')
param backendUrl string = ''

param tags object = {}

resource staticWebApp 'Microsoft.Web/staticSites@2023-01-01' = {
  name: name
  location: location
  tags: tags
  sku: {
    name: 'Free'
    tier: 'Free'
  }
  properties: {
    buildProperties: {
      appLocation: '/'
      outputLocation: 'dist'
    }
  }
}

@description('Static Web App default hostname')
output defaultHostname string = staticWebApp.properties.defaultHostname

@description('Static Web App URL')
output url string = 'https://${staticWebApp.properties.defaultHostname}'

@description('Deployment token for CI/CD')
output deploymentToken string = staticWebApp.listSecrets().properties.apiKey
