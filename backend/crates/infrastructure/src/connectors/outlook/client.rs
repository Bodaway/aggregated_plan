use async_trait::async_trait;
use chrono::NaiveDate;
use reqwest::Client;

use application::errors::ConnectorError;
use application::services::outlook_client::{OutlookClient, OutlookEvent};

pub struct GraphOutlookClient {
    http: Client,
    access_token: String,
}

impl GraphOutlookClient {
    pub fn new(access_token: String) -> Self {
        Self {
            http: Client::new(),
            access_token,
        }
    }
}

#[async_trait]
impl OutlookClient for GraphOutlookClient {
    async fn fetch_calendar(
        &self,
        _start: NaiveDate,
        _end: NaiveDate,
    ) -> Result<Vec<OutlookEvent>, ConnectorError> {
        // Implemented in Task 27
        todo!("Outlook connector not yet implemented")
    }
}
