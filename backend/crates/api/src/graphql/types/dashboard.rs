use async_graphql::Object;
use chrono::NaiveDate;

use application::use_cases::dashboard::{DailyDashboard, HalfDaySlotData, WeeklyWorkload};

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
    pub weekly_workload: WeeklyWorkloadGql,
    pub sync_statuses: Vec<SyncStatusGql>,
    pub working_hours_per_day: i32,
    pub working_days: Vec<i32>,
}

impl From<DailyDashboard> for DailyDashboardGql {
    fn from(data: DailyDashboard) -> Self {
        DailyDashboardGql {
            date: data.date,
            tasks: data.tasks.into_iter().map(TaskGql).collect(),
            meetings: data.meetings.into_iter().map(MeetingGql).collect(),
            alerts: data.alerts.into_iter().map(AlertGql).collect(),
            weekly_workload: WeeklyWorkloadGql::from(data.weekly_workload),
            sync_statuses: data.sync_statuses.into_iter().map(SyncStatusGql).collect(),
            working_hours_per_day: data.working_hours_per_day,
            working_days: data.working_days,
        }
    }
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

    async fn weekly_workload(&self) -> &WeeklyWorkloadGql {
        &self.weekly_workload
    }

    async fn sync_statuses(&self) -> &[SyncStatusGql] {
        &self.sync_statuses
    }

    /// Number of working hours per day from user configuration (default 8).
    async fn working_hours_per_day(&self) -> i32 {
        self.working_hours_per_day
    }

    /// ISO weekday numbers for working days (1=Mon … 7=Sun).
    async fn working_days(&self) -> &[i32] {
        &self.working_days
    }
}

/// Weekly workload summary.
pub struct WeeklyWorkloadGql {
    pub week_start: NaiveDate,
    pub capacity: i32,
    pub total_task_hours: f64,
    pub total_meeting_hours: f64,
    pub capacity_hours: f64,
    pub slots: Vec<HalfDaySlotGql>,
    pub working_days: Vec<i32>,
}

impl From<WeeklyWorkload> for WeeklyWorkloadGql {
    fn from(data: WeeklyWorkload) -> Self {
        let capacity_hours = data.capacity as f64 * 4.0;
        WeeklyWorkloadGql {
            week_start: data.week_start,
            capacity: data.capacity,
            total_task_hours: data.total_planned,
            total_meeting_hours: data.total_meetings,
            capacity_hours,
            slots: data.half_days.into_iter().map(HalfDaySlotGql::from).collect(),
            working_days: data.working_days,
        }
    }
}

#[Object]
impl WeeklyWorkloadGql {
    async fn week_start(&self) -> NaiveDate {
        self.week_start
    }

    /// Number of half-day slots available in the week (default 10).
    async fn capacity(&self) -> i32 {
        self.capacity
    }

    /// Total planned task hours for the week.
    async fn total_planned(&self) -> f64 {
        self.total_task_hours
    }

    /// Total meeting hours for the week.
    async fn total_meetings(&self) -> f64 {
        self.total_meeting_hours
    }

    async fn capacity_hours(&self) -> f64 {
        self.capacity_hours
    }

    /// Whether the weekly workload exceeds capacity.
    async fn overload(&self) -> bool {
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

    async fn half_days(&self) -> &[HalfDaySlotGql] {
        &self.slots
    }

    /// ISO weekday numbers for working days (1=Mon … 7=Sun).
    async fn working_days(&self) -> &[i32] {
        &self.working_days
    }
}

/// Represents a single half-day slot in the workload view.
pub struct HalfDaySlotGql {
    pub date: NaiveDate,
    pub half_day: HalfDayGql,
    pub meetings: Vec<MeetingGql>,
    pub tasks: Vec<TaskGql>,
    pub consumption: f64,
    pub is_free: bool,
}

impl From<HalfDaySlotData> for HalfDaySlotGql {
    fn from(data: HalfDaySlotData) -> Self {
        HalfDaySlotGql {
            date: data.date,
            half_day: data.half_day.into(),
            meetings: data.meetings.into_iter().map(MeetingGql).collect(),
            tasks: data.tasks.into_iter().map(TaskGql).collect(),
            consumption: data.consumption,
            is_free: data.is_free,
        }
    }
}

#[Object]
impl HalfDaySlotGql {
    async fn date(&self) -> NaiveDate {
        self.date
    }

    async fn half_day(&self) -> HalfDayGql {
        self.half_day
    }

    async fn meetings(&self) -> &[MeetingGql] {
        &self.meetings
    }

    async fn tasks(&self) -> &[TaskGql] {
        &self.tasks
    }

    /// Fraction of the half-day consumed (0.0 to 1.0+).
    async fn consumption(&self) -> f64 {
        self.consumption
    }

    /// Whether this slot is considered free (consumption < 0.5).
    async fn is_free(&self) -> bool {
        self.is_free
    }
}
