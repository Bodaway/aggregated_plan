use async_trait::async_trait;
use reqwest::Client;

use application::errors::ConnectorError;
use application::services::excel_client::{ExcelClient, ExcelMappingConfig, ExcelRow};

pub struct GraphExcelClient {
    http: Client,
    access_token: String,
}

impl GraphExcelClient {
    pub fn new(access_token: String) -> Self {
        Self {
            http: Client::new(),
            access_token,
        }
    }
}

#[async_trait]
impl ExcelClient for GraphExcelClient {
    async fn fetch_rows(
        &self,
        _config: &ExcelMappingConfig,
    ) -> Result<Vec<ExcelRow>, ConnectorError> {
        // Implemented in Task 28
        todo!("Excel connector not yet implemented")
    }
}
