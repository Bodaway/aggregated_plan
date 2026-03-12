use async_graphql::{InputObject, Object, SimpleObject, ID};
use chrono::{DateTime, NaiveDate, Utc};

use domain::types::ActivitySlot;

use super::enums::HalfDayGql;

/// Lightweight task summary returned on activity slots (stub until data loader is implemented).
#[derive(SimpleObject)]
pub struct ActivityTaskSummaryGql {
    pub id: ID,
    pub title: String,
}

/// GraphQL wrapper for the domain ActivitySlot entity.
pub struct ActivitySlotGql(pub ActivitySlot);

#[Object]
impl ActivitySlotGql {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn task_id(&self) -> Option<ID> {
        self.0.task_id.map(|tid| ID(tid.to_string()))
    }

    /// The associated task. Resolved via data loader in a later task.
    /// Returns None for now.
    async fn task(&self) -> Option<ActivityTaskSummaryGql> {
        None
    }

    async fn start_time(&self) -> DateTime<Utc> {
        self.0.start_time
    }

    async fn end_time(&self) -> Option<DateTime<Utc>> {
        self.0.end_time
    }

    async fn half_day(&self) -> HalfDayGql {
        self.0.half_day.into()
    }

    async fn date(&self) -> NaiveDate {
        self.0.date
    }

    /// Computed duration in hours (None if still active).
    async fn duration_hours(&self) -> Option<f64> {
        self.0.end_time.map(|end| {
            (end - self.0.start_time).num_minutes() as f64 / 60.0
        })
    }

    /// Computed duration in minutes (None if still active).
    async fn duration_minutes(&self) -> Option<i64> {
        self.0.end_time.map(|end| (end - self.0.start_time).num_minutes())
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }
}

/// Input for updating an existing activity slot.
#[derive(InputObject, Debug)]
pub struct UpdateActivitySlotInput {
    /// Change the associated task.
    pub task_id: Option<ID>,
}
