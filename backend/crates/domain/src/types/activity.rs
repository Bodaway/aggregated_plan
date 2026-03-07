use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySlot {
    pub id: ActivitySlotId,
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub half_day: HalfDay,
    pub date: NaiveDate,
    pub created_at: DateTime<Utc>,
}
