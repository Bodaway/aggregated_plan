use async_graphql::{Object, ID};
use chrono::{DateTime, Utc};

use domain::rules::workload::meeting_hours;
use domain::types::Meeting;

/// GraphQL wrapper for the domain Meeting entity.
pub struct MeetingGql(pub Meeting);

#[Object]
impl MeetingGql {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn title(&self) -> &str {
        &self.0.title
    }

    async fn start_time(&self) -> DateTime<Utc> {
        self.0.start_time
    }

    async fn end_time(&self) -> DateTime<Utc> {
        self.0.end_time
    }

    /// Computed duration in hours.
    async fn duration_hours(&self) -> f64 {
        meeting_hours(self.0.start_time, self.0.end_time)
    }

    /// Computed half-day consumption (0.5 or 1.0 depending on duration and time span).
    async fn half_day_consumption(&self) -> f64 {
        let hours = meeting_hours(self.0.start_time, self.0.end_time);
        if hours >= 3.5 {
            1.0
        } else {
            0.5
        }
    }

    async fn location(&self) -> Option<&str> {
        self.0.location.as_deref()
    }

    async fn participants(&self) -> &[String] {
        &self.0.participants
    }

    async fn project_id(&self) -> Option<ID> {
        self.0.project_id.map(|pid| ID(pid.to_string()))
    }

    async fn outlook_id(&self) -> &str {
        &self.0.outlook_id
    }

    async fn show_as(&self) -> Option<&str> {
        self.0.show_as.as_deref()
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }
}
