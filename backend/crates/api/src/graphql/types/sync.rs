use async_graphql::Object;
use chrono::{DateTime, Utc};

use application::repositories::SyncStatus;

use super::enums::*;

/// GraphQL wrapper for the SyncStatus entity.
pub struct SyncStatusGql(pub SyncStatus);

#[Object]
impl SyncStatusGql {
    async fn source(&self) -> SourceGql {
        self.0.source.into()
    }

    async fn last_sync_at(&self) -> Option<DateTime<Utc>> {
        self.0.last_sync_at
    }

    async fn status(&self) -> SyncSourceStatusGql {
        self.0.status.into()
    }

    async fn error_message(&self) -> Option<&str> {
        self.0.error_message.as_deref()
    }
}

/// Represents a sync event for real-time updates.
pub struct SyncEventGql {
    pub source: SourceGql,
    pub status: SyncSourceStatusGql,
    pub message: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[Object]
impl SyncEventGql {
    async fn source(&self) -> SourceGql {
        self.source
    }

    async fn status(&self) -> SyncSourceStatusGql {
        self.status
    }

    async fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    async fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}
