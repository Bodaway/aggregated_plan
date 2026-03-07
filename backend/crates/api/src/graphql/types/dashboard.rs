use async_graphql::Object;
use chrono::NaiveDate;

use super::alert::AlertGql;
use super::enums::HalfDayGql;
use super::meeting::MeetingGql;
use super::sync::SyncStatusGql;
use super::task::TaskGql;

/// Aggregated data for the daily dashboard view.
pub struct DailyDashboardGql {
    pub date: NaiveDate,
    pub tasks: Vec<TaskGql>,
    pub meetings: Vec<MeetingGql>,
    pub alerts: Vec<AlertGql>,
    pub sync_statuses: Vec<SyncStatusGql>,
}

#[Object]
impl DailyDashboardGql {
    async fn date(&self) -> NaiveDate {
        self.date
    }

    async fn tasks(&self) -> &[TaskGql] {
        &self.tasks
    }

    async fn meetings(&self) -> &[MeetingGql] {
        &self.meetings
    }

    async fn alerts(&self) -> &[AlertGql] {
        &self.alerts
    }

    async fn sync_statuses(&self) -> &[SyncStatusGql] {
        &self.sync_statuses
    }
}

/// Weekly workload summary.
pub struct WeeklyWorkloadGql {
    pub week_start: NaiveDate,
    pub total_task_hours: f64,
    pub total_meeting_hours: f64,
    pub capacity_hours: f64,
    pub slots: Vec<HalfDaySlotGql>,
}

#[Object]
impl WeeklyWorkloadGql {
    async fn week_start(&self) -> NaiveDate {
        self.week_start
    }

    async fn total_task_hours(&self) -> f64 {
        self.total_task_hours
    }

    async fn total_meeting_hours(&self) -> f64 {
        self.total_meeting_hours
    }

    async fn capacity_hours(&self) -> f64 {
        self.capacity_hours
    }

    /// Whether the weekly workload exceeds capacity.
    async fn is_overloaded(&self) -> bool {
        (self.total_task_hours + self.total_meeting_hours) > self.capacity_hours
    }

    /// Excess hours over capacity (0.0 if not overloaded).
    async fn excess_hours(&self) -> f64 {
        let total = self.total_task_hours + self.total_meeting_hours;
        if total > self.capacity_hours {
            total - self.capacity_hours
        } else {
            0.0
        }
    }

    async fn slots(&self) -> &[HalfDaySlotGql] {
        &self.slots
    }
}

/// Represents a single half-day slot in the workload view.
pub struct HalfDaySlotGql {
    pub date: NaiveDate,
    pub half_day: HalfDayGql,
    pub task_title: Option<String>,
    pub meeting_title: Option<String>,
    pub is_occupied: bool,
}

#[Object]
impl HalfDaySlotGql {
    async fn date(&self) -> NaiveDate {
        self.date
    }

    async fn half_day(&self) -> HalfDayGql {
        self.half_day
    }

    async fn task_title(&self) -> Option<&str> {
        self.task_title.as_deref()
    }

    async fn meeting_title(&self) -> Option<&str> {
        self.meeting_title.as_deref()
    }

    async fn is_occupied(&self) -> bool {
        self.is_occupied
    }
}
