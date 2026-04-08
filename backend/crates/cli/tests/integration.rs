//! End-to-end integration tests for the `aplan` binary.
//!
//! Each test stubs the GraphQL operations it needs on a `wiremock` server,
//! then invokes the binary via `assert_cmd` and asserts on stdout/stderr/exit.

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::json;
use std::process::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Start a wiremock server that responds to POST /graphql with `body` for any
/// request matching `operation_name`. Returns the mock server (so the URL stays alive).
async fn mock_graphql(body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn aplan() -> Command {
    Command::cargo_bin("aplan").unwrap()
}

#[tokio::test]
async fn current_with_running_activity_prints_one_line() {
    let server = mock_graphql(json!({
        "data": {
            "currentActivity": {
                "id": "00000000-0000-0000-0000-000000000010",
                "taskId": "00000000-0000-0000-0000-000000000001",
                "startTime": "2026-04-08T09:00:00Z",
                "halfDay": "MORNING",
                "date": "2026-04-08",
                "task": {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Auth migration"
                }
            }
        }
    }))
    .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Auth migration"))
        .stdout(predicate::str::contains("morning"));
}

#[tokio::test]
async fn current_with_no_activity_prints_placeholder() {
    let server = mock_graphql(json!({ "data": { "currentActivity": null } })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no activity running"));
}

#[tokio::test]
async fn current_with_json_flag_emits_raw_data_block() {
    let server = mock_graphql(json!({
        "data": { "currentActivity": null }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "--json", "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"currentActivity\":null"));
}

#[tokio::test]
async fn start_with_uuid_token_starts_activity() {
    let server = mock_graphql(json!({
        "data": {
            "startActivity": {
                "id": "00000000-0000-0000-0000-000000000010",
                "taskId": "00000000-0000-0000-0000-000000000001",
                "startTime": "2026-04-08T09:00:00Z",
                "halfDay": "MORNING",
                "date": "2026-04-08",
                "task": {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Auth migration"
                }
            }
        }
    }))
    .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args([
            "--api-url",
            &url,
            "start",
            "00000000-0000-0000-0000-000000000001",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("started"))
        .stdout(predicate::str::contains("Auth migration"))
        .stdout(predicate::str::contains("morning"));
}
