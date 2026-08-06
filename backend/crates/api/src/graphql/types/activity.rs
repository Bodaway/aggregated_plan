use std::sync::Arc;

use async_graphql::{Context, InputObject, MaybeUndefined, Object, SimpleObject, ID};
use chrono::{DateTime, NaiveDate, Utc};

use application::repositories::TaskRepository;
use application::use_cases::activity_tracking::ActivityOverlap;
use domain::types::{ActivitySlot, ActivitySlotId, TaskId};

use super::enums::HalfDayGql;
use super::session::SessionTaskSummaryGql;

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

    /// The associated task, resolved by looking up the task_id.
    async fn task(&self, ctx: &Context<'_>) -> Option<ActivityTaskSummaryGql> {
        let task_id = self.0.task_id?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>().ok()?;
        let task = task_repo.find_by_id(task_id).await.ok()??;
        Some(ActivityTaskSummaryGql {
            id: ID(task.id.to_string()),
            title: task.title,
        })
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

    /// Which session's work this slot projects. Null is the human.
    async fn session_id(&self) -> Option<String> {
        self.0.session_id.clone()
    }

    /// Whether the worklog projection owns this slot.
    async fn source(&self) -> super::enums::SlotSourceGql {
        self.0.source.into()
    }
}

/// Input for updating an existing activity slot.
#[derive(InputObject, Debug)]
pub struct UpdateActivitySlotInput {
    /// Change the associated task. Null clears it, undefined leaves unchanged.
    pub task_id: MaybeUndefined<ID>,
    /// Update the start time.
    pub start_time: Option<DateTime<Utc>>,
    /// Update the end time.
    pub end_time: Option<DateTime<Utc>>,
}

/// Input for creating a manual (completed) activity slot.
#[derive(InputObject, Debug)]
pub struct CreateActivitySlotInput {
    /// Start time (also determines date and half-day).
    pub start_time: DateTime<Utc>,
    /// End time (must be after start_time).
    pub end_time: DateTime<Utc>,
    /// Optional task to associate with.
    pub task_id: Option<ID>,
}

/// One side of a flagged overlap: which slot claimed the time, whose work it
/// was (`session_id`, null for the human working by hand), and the task it
/// was logged against.
///
/// `task` is nullable because `ActivitySlot::task_id` is `Option<TaskId>` —
/// but `find_overlaps` (Task 7) excludes untagged slots, so in practice this
/// is only null if the task row was later deleted, which is a data question,
/// not a reason to fail the whole query.
pub struct ActivityOverlapSideGql {
    pub slot_id: ActivitySlotId,
    pub session_id: Option<String>,
    pub task_id: Option<TaskId>,
}

#[Object]
impl ActivityOverlapSideGql {
    async fn slot_id(&self) -> ID {
        ID(self.slot_id.to_string())
    }

    async fn session_id(&self) -> Option<String> {
        self.session_id.clone()
    }

    /// The task this side's slot was logged against, resolved by looking up
    /// `task_id`. Reuses `SessionTaskSummaryGql` rather than declaring a
    /// second `{ id, title }` type — async-graphql registers type names
    /// globally, and this repository has already paid for one collision.
    async fn task(&self, ctx: &Context<'_>) -> Option<SessionTaskSummaryGql> {
        let task_id = self.task_id?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>().ok()?;
        let task = task_repo.find_by_id(task_id).await.ok()??;
        Some(SessionTaskSummaryGql {
            id: ID(task.id.to_string()),
            title: task.title,
        })
    }
}

/// A pair of different tasks' slots claiming overlapping time, and how many
/// minutes they share. Nothing here corrects the double count — see
/// `domain::rules::overlap` — it only reports it; the user arbitrates at the
/// timesheet review.
#[derive(SimpleObject)]
pub struct ActivityOverlapGql {
    pub minutes: i64,
    pub a: ActivityOverlapSideGql,
    pub b: ActivityOverlapSideGql,
}

impl From<ActivityOverlap> for ActivityOverlapGql {
    fn from(overlap: ActivityOverlap) -> Self {
        Self {
            minutes: overlap.minutes,
            a: ActivityOverlapSideGql {
                slot_id: overlap.a.id,
                session_id: overlap.a.session_id,
                task_id: overlap.a.task_id,
            },
            b: ActivityOverlapSideGql {
                slot_id: overlap.b.id,
                session_id: overlap.b.session_id,
                task_id: overlap.b.task_id,
            },
        }
    }
}
