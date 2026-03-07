use serde::{Deserialize, Serialize};

use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub user_id: UserId,
    pub name: String,
    pub color: Option<String>,
}
