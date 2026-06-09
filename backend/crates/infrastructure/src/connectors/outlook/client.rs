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

/// Maximum number of pagination iterations to guard against an infinite loop.
const MAX_PAGINATION_PAGES: usize = 50;

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

        // Fetch the first page with $top=100 to reduce round-trips.
        let first_response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .header("Accept", "application/json")
            .query(&[
                ("startDateTime", start_dt.as_str()),
                ("endDateTime", end_dt.as_str()),
                (
                    "$select",
                    "id,subject,start,end,location,attendees,isCancelled,showAs",
                ),
                ("$top", "100"),
            ])
            .send()
            .await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;

        let first_page = Self::parse_page(first_response).await?;

        let mut all_graph_events = first_page.value;
        let mut next_link = first_page.odata_next_link;
        let mut pages_fetched: usize = 1;

        // Follow @odata.nextLink cursors until exhausted or safety cap reached.
        while let Some(ref link) = next_link.clone() {
            if pages_fetched >= MAX_PAGINATION_PAGES {
                tracing::warn!(
                    "fetch_calendar: pagination safety cap ({} pages) reached; some events may be missing",
                    MAX_PAGINATION_PAGES
                );
                break;
            }

            // The nextLink URL is absolute and already contains all query params
            // ($skiptoken etc.). Only attach auth headers.
            let page_response = self
                .http
                .get(link.as_str())
                .header("Authorization", format!("Bearer {}", self.access_token))
                .header("Accept", "application/json")
                .send()
                .await
                .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;

            let page = Self::parse_page(page_response).await?;
            all_graph_events.extend(page.value);
            next_link = page.odata_next_link;
            pages_fetched += 1;
        }

        // Map events and filter out cancelled ones and those that fail to parse.
        let events: Vec<OutlookEvent> = all_graph_events
            .into_iter()
            .filter(|e| !e.is_cancelled)
            .filter_map(map_graph_event)
            .collect();

        Ok(events)
    }
}

impl GraphOutlookClient {
    /// Checks the HTTP status of a Graph API response and deserialises it as
    /// [`GraphCalendarResponse`]. Errors are mapped to [`ConnectorError`].
    async fn parse_page(
        response: reqwest::Response,
    ) -> Result<GraphCalendarResponse, ConnectorError> {
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
        response
            .json::<GraphCalendarResponse>()
            .await
            .map_err(|e| ConnectorError::ParseError(e.to_string()))
    }
}
