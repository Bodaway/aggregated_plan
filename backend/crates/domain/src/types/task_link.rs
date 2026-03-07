use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLink {
    pub id: TaskLinkId,
    pub task_id_primary: TaskId,
    pub task_id_secondary: TaskId,
    pub link_type: TaskLinkType,
    pub confidence_score: Option<f64>,
    pub created_at: DateTime<Utc>,
}
