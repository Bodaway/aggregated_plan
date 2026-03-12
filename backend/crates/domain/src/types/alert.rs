use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: AlertId,
    pub user_id: UserId,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub related_items: Vec<RelatedItem>,
    pub date: NaiveDate,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelatedItem {
    Task(TaskId),
    Meeting(MeetingId),
}
