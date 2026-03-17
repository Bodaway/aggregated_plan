use async_graphql::{Object, SimpleObject, ID};
use chrono::NaiveDate;

use application::use_cases::activity_reporting::WeeklyActivitySummary;

/// GraphQL wrapper for weekly activity summary.
pub struct WeeklyActivitySummaryGql(pub WeeklyActivitySummary);

#[Object]
impl WeeklyActivitySummaryGql {
    async fn week_start(&self) -> NaiveDate {
        self.0.week_start
    }

    async fn week_end(&self) -> NaiveDate {
        self.0.week_end
    }

    async fn total_hours(&self) -> f64 {
        self.0.total_hours
    }

    async fn daily_totals(&self) -> Vec<DailyActivityTotalGql> {
        self.0
            .daily_totals
            .iter()
            .map(|d| DailyActivityTotalGql {
                date: d.date,
                total_hours: d.total_hours,
            })
            .collect()
    }

    async fn task_breakdown(&self) -> Vec<TaskActivitySummaryGql> {
        self.0
            .task_breakdown
            .iter()
            .map(|t| TaskActivitySummaryGql {
                task_id: t.task_id.map(|id| ID(id.to_string())),
                task_title: t.task_title.clone(),
                source_id: t.source_id.clone(),
                total_hours: t.total_hours,
                daily_hours: t.daily_hours.clone(),
            })
            .collect()
    }
}

/// Daily total tracked hours.
#[derive(SimpleObject)]
pub struct DailyActivityTotalGql {
    pub date: NaiveDate,
    pub total_hours: f64,
}

/// Per-task breakdown for the week.
pub struct TaskActivitySummaryGql {
    pub task_id: Option<ID>,
    pub task_title: Option<String>,
    pub source_id: Option<String>,
    pub total_hours: f64,
    pub daily_hours: Vec<f64>,
}

#[Object]
impl TaskActivitySummaryGql {
    async fn task_id(&self) -> &Option<ID> {
        &self.task_id
    }

    async fn task_title(&self) -> &Option<String> {
        &self.task_title
    }

    async fn source_id(&self) -> &Option<String> {
        &self.source_id
    }

    async fn total_hours(&self) -> f64 {
        self.total_hours
    }

    async fn daily_hours(&self) -> &[f64] {
        &self.daily_hours
    }
}
