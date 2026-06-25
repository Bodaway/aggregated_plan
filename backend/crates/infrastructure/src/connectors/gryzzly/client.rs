use std::time::Duration;

use application::errors::ConnectorError;
use application::services::{GryzzlyClient, GryzzlyProject, GryzzlyTask};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};

use super::mapper::{map_project, map_task};
use super::types::{RawGryzzlyProject, RawGryzzlyTask, RawList};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SERVICE: &str = "gryzzly";

pub struct HttpGryzzlyClient {
    http: Client,
    base_url: String,
    api_key: String,
}

impl HttpGryzzlyClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build reqwest client");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ConnectorError> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;

        match resp.status() {
            s if s.is_success() => resp
                .json::<T>()
                .await
                .map_err(|e| ConnectorError::ParseError(e.to_string())),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(ConnectorError::AuthFailed { service: SERVICE.to_string() })
            }
            // Rate limited: surface as Http{429,..}. A bounded Retry-After-aware
            // retry can be added here once the real limit is known.
            status => {
                let code = status.as_u16();
                let body = resp.text().await.unwrap_or_else(|e| e.to_string());
                Err(ConnectorError::Http { status: code, message: body })
            }
        }
    }
}

#[async_trait]
impl GryzzlyClient for HttpGryzzlyClient {
    async fn fetch_projects(&self, active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError> {
        // Endpoint + pagination are placeholders pending the prerequisite probe.
        let page: RawList<RawGryzzlyProject> = self.get_json("projects?limit=1000").await?;
        let mut projects: Vec<GryzzlyProject> = page.data.into_iter().map(map_project).collect();
        if active_only {
            projects.retain(|p| p.is_active);
        }
        Ok(projects)
    }

    async fn fetch_tasks(&self, project_ids: &[String]) -> Result<Vec<GryzzlyTask>, ConnectorError> {
        let mut out = Vec::new();
        for project_id in project_ids {
            let page: RawList<RawGryzzlyTask> =
                self.get_json(&format!("tasks?project_id={}&limit=1000", project_id)).await?;
            // project_active is true here because callers pass only active project ids;
            // if the task API has its own flag, map_task already ANDs it in.
            out.extend(page.data.into_iter().map(|t| map_task(t, true)));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_trims_trailing_slash_and_keeps_key() {
        let c = HttpGryzzlyClient::new("https://api.gryzzly.io/v1/".into(), "secret".into());
        assert_eq!(c.base_url, "https://api.gryzzly.io/v1");
        assert_eq!(c.api_key, "secret");
    }
}
