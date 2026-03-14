use serde::{Deserialize, Serialize};

use super::common::TaskId;

/// Result from a full-text search query, including which field matched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSearchResult {
    pub task_id: TaskId,
    pub matched_field: String,
    pub matched_snippet: String,
}
