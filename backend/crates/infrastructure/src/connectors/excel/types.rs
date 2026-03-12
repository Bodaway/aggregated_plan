use serde::Deserialize;

/// Microsoft Graph API Excel/SharePoint response types.

#[derive(Debug, Deserialize)]
pub struct GraphWorksheetRange {
    pub values: Vec<Vec<serde_json::Value>>,
}
