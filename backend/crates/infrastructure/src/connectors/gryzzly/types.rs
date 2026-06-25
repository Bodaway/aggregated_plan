use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RawGryzzlyProject {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub customer_name: Option<String>,
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RawGryzzlyTask {
    pub id: String,
    pub name: String,
    pub project_id: String,
    #[serde(default)]
    pub archived: Option<bool>,
}

/// Envelope for a paginated list response. Replace with the real pagination shape
/// (cursor token vs offset) confirmed in the prerequisite probe.
#[derive(Debug, Clone, Deserialize)]
pub struct RawList<T> {
    pub data: Vec<T>,
}
