use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}
