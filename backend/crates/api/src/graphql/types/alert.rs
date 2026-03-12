use async_graphql::{Object, SimpleObject, ID};
use chrono::{DateTime, NaiveDate, Utc};

use domain::types::{Alert, RelatedItem};

use super::enums::*;
use super::pagination::PageInfo;

/// GraphQL wrapper for a related item in an alert (task or meeting reference).
pub struct RelatedItemGql(pub RelatedItem);

#[Object]
impl RelatedItemGql {
    /// The type of the related item: "TASK" or "MEETING".
    async fn item_type(&self) -> &str {
        match &self.0 {
            RelatedItem::Task(_) => "TASK",
            RelatedItem::Meeting(_) => "MEETING",
        }
    }

    /// The ID of the related item.
    async fn item_id(&self) -> ID {
        match &self.0 {
            RelatedItem::Task(id) => ID(id.to_string()),
            RelatedItem::Meeting(id) => ID(id.to_string()),
        }
    }
}

/// GraphQL wrapper for the domain Alert entity.
pub struct AlertGql(pub Alert);

#[Object]
impl AlertGql {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn alert_type(&self) -> AlertTypeGql {
        self.0.alert_type.into()
    }

    async fn severity(&self) -> AlertSeverityGql {
        self.0.severity.into()
    }

    async fn message(&self) -> &str {
        &self.0.message
    }

    async fn related_items(&self) -> Vec<RelatedItemGql> {
        self.0
            .related_items
            .iter()
            .cloned()
            .map(RelatedItemGql)
            .collect()
    }

    async fn date(&self) -> NaiveDate {
        self.0.date
    }

    async fn resolved(&self) -> bool {
        self.0.resolved
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }
}

/// Relay-style edge for alert pagination.
pub struct AlertEdge {
    pub node: AlertGql,
    pub cursor: String,
}

#[Object]
impl AlertEdge {
    async fn node(&self) -> &AlertGql {
        &self.node
    }

    async fn cursor(&self) -> &str {
        &self.cursor
    }
}

/// Relay-style connection for paginated alert results.
#[derive(SimpleObject)]
pub struct AlertConnection {
    pub edges: Vec<AlertEdge>,
    pub page_info: PageInfo,
    pub total_count: i32,
}
