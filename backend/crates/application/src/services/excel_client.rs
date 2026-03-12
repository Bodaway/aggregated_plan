use async_trait::async_trait;
use std::collections::HashMap;

use crate::errors::ConnectorError;

/// Represents a single row from an Excel spreadsheet.
pub struct ExcelRow {
    pub row_index: usize,
    /// Column name to cell value mapping.
    pub columns: HashMap<String, String>,
}

/// Configuration for mapping Excel columns to task fields.
pub struct ExcelMappingConfig {
    pub sharepoint_path: String,
    pub sheet_name: Option<String>,
    pub title_column: String,
    pub assignee_column: Option<String>,
    pub project_column: Option<String>,
    pub date_column: Option<String>,
    pub jira_key_column: Option<String>,
    pub status_column: Option<String>,
}

/// Client trait for fetching rows from an Excel/SharePoint spreadsheet.
#[async_trait]
pub trait ExcelClient: Send + Sync {
    /// Fetch rows from the Excel spreadsheet using the provided mapping config.
    async fn fetch_rows(
        &self,
        config: &ExcelMappingConfig,
    ) -> Result<Vec<ExcelRow>, ConnectorError>;
}
