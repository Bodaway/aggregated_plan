@description('Name of the storage account (3-24 lowercase alphanumeric)')
param name string

@description('Location for the resource')
param location string

@description('Name of the file share for SQLite data')
param fileShareName string = 'sqlitedata'

param tags object = {}

resource storageAccount 'Microsoft.Storage/storageAccounts@2023-01-01' = {
  name: name
  location: location
  tags: tags
  kind: 'StorageV2'
  sku: {
    name: 'Standard_LRS'
  }
  properties: {
    minimumTlsVersion: 'TLS1_2'
    allowBlobPublicAccess: false
  }
}

resource fileService 'Microsoft.Storage/storageAccounts/fileServices@2023-01-01' = {
  parent: storageAccount
  name: 'default'
}

resource fileShare 'Microsoft.Storage/storageAccounts/fileServices/shares@2023-01-01' = {
  parent: fileService
  name: fileShareName
  properties: {
    shareQuota: 1 // 1 GB - more than enough for SQLite
  }
}

@description('Storage account name')
output storageAccountName string = storageAccount.name

@description('Storage account key')
output storageAccountKey string = storageAccount.listKeys().keys[0].value

@description('File share name')
output fileShareName string = fileShare.name
