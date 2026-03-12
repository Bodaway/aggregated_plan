use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};

use crate::errors::ConnectorError;

/// Represents a calendar event fetched from Outlook.
pub struct OutlookEvent {
    pub outlook_id: String,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub location: Option<String>,
    pub participants: Vec<String>,
    pub is_cancelled: bool,
    pub show_as: Option<String>,
}

/// Client trait for fetching calendar events from Outlook.
#[async_trait]
pub trait OutlookClient: Send + Sync {
    /// Fetch calendar events within the given date range.
    async fn fetch_calendar(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<OutlookEvent>, ConnectorError>;
}
