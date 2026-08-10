//! HTTP client for the Gryzzly internal API.
//!
//! The API is RPC-style: every method is `POST https://api.gryzzly.io/<method>`
//! with a JSON body and a `{ok, payload}` envelope — reads included. Auth is
//! `Authorization: User <session-token>`; Gryzzly issues no API keys.
//!
//! Read-only by construction: only `view/projects.list` and
//! `expandedProjectMetrics.get` are ever called.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use application::errors::ConnectorError;
use application::services::{GryzzlyClient, GryzzlyProject, GryzzlyTask, GryzzlyTokenSource};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tokio::sync::OnceCell;

use super::mapper::{flatten_tasks, map_project, map_task};
use super::types::{Envelope, RawGryzzlyProject, RawProjectMetrics};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SERVICE: &str = "gryzzly";

/// `view/projects.list` rejects anything above 500.
const PROJECTS_LIMIT: u32 = 500;

/// Page guard for the cursor walk: 200 pages × 500 = 100k projects. It exists so
/// a server-side cursor that never nulls fails loudly instead of hanging a sync.
const MAX_PROJECT_PAGES: usize = 200;

pub struct HttpGryzzlyClient {
    http: Client,
    base_url: String,
    tokens: Arc<dyn GryzzlyTokenSource>,
    /// The token is read once per client. A client lives for one sync, so this
    /// keeps a ~20-call sync from spawning `secret-tool` twenty times, while
    /// still picking up a fresh cookie on the next sync.
    header: OnceCell<String>,
}

impl HttpGryzzlyClient {
    pub fn new(base_url: String, tokens: Arc<dyn GryzzlyTokenSource>) -> Self {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build reqwest client");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            tokens,
            header: OnceCell::new(),
        }
    }

    async fn auth_header(&self) -> Result<&String, ConnectorError> {
        self.header
            .get_or_try_init(|| async { self.tokens.header_value().await })
            .await
    }

    /// Turn a failed response body into the most useful message available: the
    /// API's own `errors` array if it parses, else the raw body.
    fn error_message(body: &str) -> String {
        serde_json::from_str::<Envelope<Value>>(body)
            .ok()
            .and_then(|e| e.errors)
            .map(|errs| errs.join("; "))
            .unwrap_or_else(|| body.to_string())
    }

    async fn post_envelope<T: DeserializeOwned>(
        &self,
        method: &str,
        body: &Value,
    ) -> Result<Envelope<T>, ConnectorError> {
        let url = format!("{}/{}", self.base_url, method.trim_start_matches('/'));
        let auth = self.auth_header().await?.clone();
        let resp = self
            .http
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;

        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(ConnectorError::AuthFailed { service: SERVICE.to_string() });
        }

        let text = resp
            .text()
            .await
            .map_err(|e| ConnectorError::ParseError(e.to_string()))?;

        if !status.is_success() {
            return Err(ConnectorError::Http {
                status: status.as_u16(),
                message: Self::error_message(&text),
            });
        }

        let envelope: Envelope<T> = serde_json::from_str(&text)
            .map_err(|e| ConnectorError::ParseError(format!("{method}: {e}")))?;
        if !envelope.ok {
            return Err(ConnectorError::Http {
                status: status.as_u16(),
                message: Self::error_message(&text),
            });
        }
        Ok(envelope)
    }

    async fn post_payload<T: DeserializeOwned + Default>(
        &self,
        method: &str,
        body: &Value,
    ) -> Result<T, ConnectorError> {
        Ok(self.post_envelope::<T>(method, body).await?.payload.unwrap_or_default())
    }
}

#[async_trait]
impl GryzzlyClient for HttpGryzzlyClient {
    async fn fetch_projects(&self, active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError> {
        let mut projects: Vec<GryzzlyProject> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut cursor: Option<String> = None;
        let mut pages = 0usize;

        loop {
            if pages >= MAX_PROJECT_PAGES {
                return Err(ConnectorError::Configuration(format!(
                    "view/projects.list did not finish paginating after {MAX_PROJECT_PAGES} pages \
                     — the API kept returning a cursor"
                )));
            }
            let mut body = json!({
                "filter": "", "range": "", "search": "", "limit": PROJECTS_LIMIT
            });
            if let Some(c) = &cursor {
                body["cursor"] = json!(c);
            }

            let envelope: Envelope<Vec<RawGryzzlyProject>> =
                self.post_envelope("view/projects.list", &body).await?;
            pages += 1;

            for raw in envelope.payload.unwrap_or_default() {
                if seen.insert(raw.id.clone()) {
                    projects.push(map_project(raw));
                }
            }

            // `limit` is a pre-filter batch size, so a short or even empty page
            // says nothing about whether more data follows. Only a null (or
            // empty) cursor ends the walk.
            match envelope.cursor.filter(|c| !c.is_empty()) {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }

        if active_only {
            projects.retain(|p| p.is_active);
        }
        Ok(projects)
    }

    async fn fetch_tasks(&self, project_ids: &[String]) -> Result<Vec<GryzzlyTask>, ConnectorError> {
        let mut out = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for project_id in project_ids {
            let body = json!({"project_id": project_id});
            let metrics: RawProjectMetrics = self
                .post_payload("expandedProjectMetrics.get", &body)
                .await?;
            let flat = flatten_tasks(metrics.tasks.unwrap_or_default(), project_id, 0);
            for raw in flat {
                // Callers pass only active project ids, so project_active is true.
                let task = map_task(raw, true);
                if seen.insert(task.id.clone()) {
                    out.push(task);
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct FixedToken(&'static str);

    #[async_trait]
    impl GryzzlyTokenSource for FixedToken {
        async fn header_value(&self) -> Result<String, ConnectorError> {
            Ok(self.0.to_string())
        }
    }

    struct FailingToken;

    #[async_trait]
    impl GryzzlyTokenSource for FailingToken {
        async fn header_value(&self) -> Result<String, ConnectorError> {
            Err(ConnectorError::Configuration("no cookie".into()))
        }
    }

    fn client(server: &MockServer) -> HttpGryzzlyClient {
        HttpGryzzlyClient::new(server.uri(), Arc::new(FixedToken("User tok123")))
    }

    #[test]
    fn new_trims_a_trailing_slash() {
        let c = HttpGryzzlyClient::new(
            "https://api.gryzzly.io/".into(),
            Arc::new(FixedToken("User t")),
        );
        assert_eq!(c.base_url, "https://api.gryzzly.io");
    }

    /// Reads are POSTs here: the internal API is RPC-style. Getting this wrong
    /// is a 404, not a compile error, so it is pinned by a test.
    #[tokio::test]
    async fn posts_to_the_method_path_with_the_user_auth_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/view/projects.list"))
            .and(header("authorization", "User tok123"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "cursor": null,
                "payload": [{"id": "p1", "name": "Website", "status": "active"}]
            })))
            .mount(&server)
            .await;

        let got: Vec<RawGryzzlyProject> = client(&server)
            .post_payload("view/projects.list", &json!({"limit": 500}))
            .await
            .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "p1");
    }

    #[tokio::test]
    async fn a_401_is_an_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let err = client(&server)
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ConnectorError::AuthFailed { .. }), "got {err:?}");
    }

    #[tokio::test]
    async fn a_403_is_an_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let err = client(&server)
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ConnectorError::AuthFailed { .. }), "got {err:?}");
    }

    /// The real API answers a bad `limit` with HTTP 400 AND an `errors` array.
    /// The array is the useful part, so it must survive into the message.
    #[tokio::test]
    async fn a_400_surfaces_the_errors_array() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "ok": false,
                "errors": ["decoding: invalid_argument: limit (out of range, max=500)"]
            })))
            .mount(&server)
            .await;

        let err = client(&server)
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({"limit": 1000}))
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("max=500"), "errors array lost: {msg}");
        assert!(matches!(err, ConnectorError::Http { status: 400, .. }), "got {err:?}");
    }

    /// `ok: false` under a 200 must not be read as success.
    #[tokio::test]
    async fn a_200_with_ok_false_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": false, "errors": ["internal server error"]
            })))
            .mount(&server)
            .await;

        let err = client(&server)
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("internal server error"), "got {err}");
    }

    #[tokio::test]
    async fn a_token_source_failure_stops_before_any_request() {
        let server = MockServer::start().await;
        // No mock mounted: any request would 404 and fail differently.
        let c = HttpGryzzlyClient::new(server.uri(), Arc::new(FailingToken));
        let err = c
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ConnectorError::Configuration(_)), "got {err:?}");
    }

    /// The token is read once per client, not once per request: a sync makes
    /// ~20 calls and each cookie read spawns secret-tool.
    #[tokio::test]
    async fn the_token_is_fetched_once_per_client() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Counting(Arc<AtomicUsize>);

        #[async_trait]
        impl GryzzlyTokenSource for Counting {
            async fn header_value(&self) -> Result<String, ConnectorError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("User tok123".to_string())
            }
        }

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "cursor": null, "payload": []
            })))
            .mount(&server)
            .await;

        let calls = Arc::new(AtomicUsize::new(0));
        let c = HttpGryzzlyClient::new(server.uri(), Arc::new(Counting(calls.clone())));
        for _ in 0..3 {
            let _: Vec<RawGryzzlyProject> =
                c.post_payload("view/projects.list", &json!({})).await.unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    // --- Cursor pagination -------------------------------------------------

    use std::sync::Mutex;
    use wiremock::{Request, Respond};

    /// Serves a scripted sequence of responses and records the request bodies,
    /// so a test can assert both the walk's results and the cursor it echoed.
    struct ScriptedPages {
        pages: Mutex<std::collections::VecDeque<serde_json::Value>>,
        seen: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    impl Respond for ScriptedPages {
        fn respond(&self, req: &Request) -> ResponseTemplate {
            self.seen
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&req.body).unwrap_or(json!(null)));
            let page = self
                .pages
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| json!({"ok": true, "payload": [], "cursor": null}));
            ResponseTemplate::new(200).set_body_json(page)
        }
    }

    fn page(ids: &[&str], cursor: Option<&str>) -> serde_json::Value {
        let payload: Vec<serde_json::Value> = ids
            .iter()
            .map(|id| json!({"id": id, "name": id, "status": "active"}))
            .collect();
        json!({"ok": true, "payload": payload, "cursor": cursor})
    }

    async fn walk_with(pages: Vec<serde_json::Value>) -> (Vec<GryzzlyProject>, Vec<serde_json::Value>) {
        let server = MockServer::start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        Mock::given(method("POST"))
            .and(path("/view/projects.list"))
            .respond_with(ScriptedPages {
                pages: Mutex::new(pages.into_iter().collect()),
                seen: seen.clone(),
            })
            .mount(&server)
            .await;

        let got = client(&server).fetch_projects(false).await.unwrap();
        let bodies = seen.lock().unwrap().clone();
        (got, bodies)
    }

    /// THE regression test. Every page is shorter than `limit` because `limit` is
    /// a pre-filter batch size — stopping on a short page would return only "a".
    #[tokio::test]
    async fn short_pages_do_not_end_the_walk() {
        let (got, bodies) = walk_with(vec![
            page(&["a"], Some("c1")),
            page(&["b", "c"], Some("c2")),
            page(&["d"], None),
        ])
        .await;

        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c", "d"]);
        assert_eq!(bodies.len(), 3);
        assert_eq!(bodies[0].get("cursor"), None, "first request must not send a cursor");
        assert_eq!(bodies[1]["cursor"], json!("c1"));
        assert_eq!(bodies[2]["cursor"], json!("c2"));
        assert_eq!(bodies[0]["limit"], json!(500));
    }

    /// An empty page with a live cursor is not the end either: limit=2 really
    /// does return zero projects mid-walk.
    #[tokio::test]
    async fn an_empty_page_with_a_cursor_continues() {
        let (got, bodies) = walk_with(vec![
            page(&["a"], Some("c1")),
            page(&[], Some("c2")),
            page(&["b"], None),
        ])
        .await;

        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
        assert_eq!(bodies.len(), 3);
    }

    #[tokio::test]
    async fn a_single_page_with_a_null_cursor_makes_one_call() {
        let (got, bodies) = walk_with(vec![page(&["a", "b"], None)]).await;
        assert_eq!(got.len(), 2);
        assert_eq!(bodies.len(), 1);
    }

    /// An empty cursor string is as final as a null one.
    #[tokio::test]
    async fn an_empty_cursor_string_ends_the_walk() {
        let (got, bodies) = walk_with(vec![page(&["a"], Some(""))]).await;
        assert_eq!(got.len(), 1);
        assert_eq!(bodies.len(), 1);
    }

    #[tokio::test]
    async fn repeated_ids_across_pages_are_not_double_counted() {
        let (got, _) = walk_with(vec![
            page(&["a", "b"], Some("c1")),
            page(&["b", "c"], None),
        ])
        .await;

        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    /// A cursor that never nulls must error, not spin forever.
    #[tokio::test]
    async fn a_never_ending_cursor_hits_the_page_guard() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/view/projects.list"))
            // Always a fresh cursor, never null.
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "payload": [{"id": "a", "name": "a", "status": "active"}],
                "cursor": "forever"
            })))
            .mount(&server)
            .await;

        let err = client(&server).fetch_projects(false).await.unwrap_err();
        assert!(matches!(err, ConnectorError::Configuration(_)), "got {err:?}");
        assert!(err.to_string().contains("200"), "guard limit missing: {err}");
    }

    #[tokio::test]
    async fn active_only_filters_after_the_walk() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/view/projects.list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "cursor": null,
                "payload": [
                    {"id": "a", "name": "a", "status": "active"},
                    {"id": "b", "name": "b", "status": "done"}
                ]
            })))
            .mount(&server)
            .await;

        let got = client(&server).fetch_projects(true).await.unwrap();
        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a"]);
    }
}
