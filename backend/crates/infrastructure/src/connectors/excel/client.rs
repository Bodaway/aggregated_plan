use async_trait::async_trait;
use reqwest::Client;

use application::errors::ConnectorError;
use application::services::excel_client::{ExcelClient, ExcelMappingConfig, ExcelRow};

use super::mapper::map_worksheet_range;
use super::types::GraphWorksheetRange;

const GRAPH_BASE_URL: &str = "https://graph.microsoft.com/v1.0";

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
        config: &ExcelMappingConfig,
    ) -> Result<Vec<ExcelRow>, ConnectorError> {
        let sheet = config.sheet_name.as_deref().unwrap_or("Sheet1");
        let path = &config.sharepoint_path;

        let url = format!(
            "{}/me/drive/root:/{path}:/workbook/worksheets('{sheet}')/usedRange",
            GRAPH_BASE_URL,
        );

        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ConnectorError::AuthFailed {
                service: "Excel/SharePoint".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ConnectorError::Http {
                status: status.as_u16(),
                message: body,
            });
        }

        let range: GraphWorksheetRange = response
            .json()
            .await
            .map_err(|e| ConnectorError::ParseError(e.to_string()))?;

        Ok(map_worksheet_range(range, config))
    }
}
