//! Thin wrapper around `reqwest::blocking` for posting GraphQL operations
//! built by `graphql_client`. Returns both the typed `ResponseData` and the
//! raw JSON `data` block, so commands can format human output from the typed
//! struct and emit the raw payload directly in `--json` mode without needing
//! the response types to implement `Serialize`.

use graphql_client::GraphQLQuery;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("cannot reach API at {url}\nhint: is the backend running? try `cargo run -p api`")]
    Unreachable { url: String },

    #[error("HTTP {status} from API: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("GraphQL error: {0}")]
    Graphql(String),

    #[error("response payload missing `data` block")]
    NoData,

    #[error("transport error: {0}")]
    Transport(String),
}

/// Result of a successful GraphQL operation: typed view + raw JSON `data` block.
pub struct RunResult<T> {
    pub data: T,
    pub raw: serde_json::Value,
}

pub struct Client {
    inner: reqwest::blocking::Client,
    api_url: String,
}

impl Client {
    pub fn new(api_url: String) -> Self {
        let inner = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("reqwest client builds with default config");
        Self { inner, api_url }
    }

    #[allow(dead_code)]
    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    /// Issue a GraphQL operation. Returns the parsed `data` block both as the
    /// typed `Q::ResponseData` and as a raw `serde_json::Value`. Errors cover
    /// network failures, non-2xx responses, and any GraphQL `errors`.
    pub fn run<Q: GraphQLQuery>(
        &self,
        variables: Q::Variables,
    ) -> Result<RunResult<Q::ResponseData>, ClientError> {
        let body = Q::build_query(variables);

        let response = self
            .inner
            .post(&self.api_url)
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    ClientError::Unreachable {
                        url: self.api_url.clone(),
                    }
                } else {
                    ClientError::Transport(e.to_string())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(ClientError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        // Parse the body once into a Value, surface any GraphQL errors, then
        // re-deserialize the `data` block into the typed Q::ResponseData.
        let envelope: serde_json::Value = response
            .json()
            .map_err(|e| ClientError::Transport(e.to_string()))?;

        if let Some(errors) = envelope.get("errors").and_then(|e| e.as_array()) {
            if !errors.is_empty() {
                let messages: Vec<String> = errors
                    .iter()
                    .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                    .map(|s| s.to_string())
                    .collect();
                return Err(ClientError::Graphql(messages.join("; ")));
            }
        }

        let data_value = envelope
            .get("data")
            .cloned()
            .ok_or(ClientError::NoData)?;

        let typed: Q::ResponseData = serde_json::from_value(data_value.clone())
            .map_err(|e| ClientError::Transport(e.to_string()))?;

        Ok(RunResult {
            data: typed,
            raw: data_value,
        })
    }
}
