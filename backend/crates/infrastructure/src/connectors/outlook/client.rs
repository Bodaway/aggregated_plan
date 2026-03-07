use async_trait::async_trait;
use chrono::NaiveDate;
use reqwest::Client;

use application::errors::ConnectorError;
use application::services::outlook_client::{OutlookClient, OutlookEvent};

use super::mapper::map_graph_event;
use super::types::GraphCalendarResponse;

const GRAPH_BASE_URL: &str = "https://graph.microsoft.com/v1.0";

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
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<OutlookEvent>, ConnectorError> {
        let start_dt = format!("{}T00:00:00Z", start);
        let end_dt = format!("{}T23:59:59Z", end);

        let url = format!("{}/me/calendarView", GRAPH_BASE_URL);

        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .header("Accept", "application/json")
            .query(&[
                ("startDateTime", start_dt.as_str()),
                ("endDateTime", end_dt.as_str()),
                (
                    "$select",
                    "id,subject,start,end,location,attendees,isCancelled",
                ),
            ])
            .send()
            .await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ConnectorError::AuthFailed {
                service: "Outlook".to_string(),
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ConnectorError::Http {
                status: status.as_u16(),
                message: body,
            });
        }

        let calendar_response: GraphCalendarResponse = response
            .json()
            .await
            .map_err(|e| ConnectorError::ParseError(e.to_string()))?;

        // Map events and filter out cancelled ones and those that fail to parse.
        let events: Vec<OutlookEvent> = calendar_response
            .value
            .into_iter()
            .filter(|e| !e.is_cancelled)
            .filter_map(map_graph_event)
            .collect();

        Ok(events)
    }
}
