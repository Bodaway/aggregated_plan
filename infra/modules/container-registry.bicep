@description('Name of the container registry')
param name string

@description('Location for the resource')
param location string

@description('SKU - Basic is cheapest')
@allowed(['Basic', 'Standard', 'Premium'])
param sku string = 'Basic'

param tags object = {}

resource acr 'Microsoft.ContainerRegistry/registries@2023-07-01' = {
  name: name
  location: location
  tags: tags
  sku: {
    name: sku
  }
  properties: {
    adminUserEnabled: true
  }
}

@description('Login server URL')
output loginServer string = acr.properties.loginServer

@description('Registry resource ID')
output id string = acr.id

@description('Registry name')
output registryName string = acr.name
