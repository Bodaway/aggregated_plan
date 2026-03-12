use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: MeetingId,
    pub user_id: UserId,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub location: Option<String>,
    pub participants: Vec<String>,
    pub project_id: Option<ProjectId>,
    pub outlook_id: String,
    pub show_as: Option<String>,
    pub created_at: DateTime<Utc>,
}
