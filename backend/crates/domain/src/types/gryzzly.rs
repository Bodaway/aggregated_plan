use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::UserId;

/// One row of the Gryzzly catalog cache (a Gryzzly task + denormalized project info).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GryzzlyCatalogEntry {
    pub id: Uuid,
    pub user_id: UserId,
    pub gryzzly_task_id: String,
    pub name: String,
    pub gryzzly_project_id: String,
    pub project_name: String,
    pub customer_name: Option<String>,
    pub is_active: bool,
    /// Status of the owning Gryzzly project, verbatim from the API: `active` or
    /// `done`. `None` means unknown — a row written before the column existed —
    /// and is read as active, never as terminated.
    pub project_status: Option<String>,
    pub last_synced_at: DateTime<Utc>,
}
