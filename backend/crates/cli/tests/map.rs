//! Integration tests for `aplan map add` / `aplan map list`.
//!
//! Mirrors the setup in `tests/integration.rs` / `tests/timesheet.rs`: a
//! `wiremock` server stubs the GraphQL operations the command issues, and
//! `assert_cmd` drives the real `aplan` binary, asserting on
//! stdout/stderr/exit code (and, where it matters, the outbound GraphQL
//! variables recorded by the mock server).

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::json;
use std::process::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Start a wiremock server that responds to POST /graphql with `body` for any
/// request. Returns the mock server (so the URL stays alive).
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

fn learn_mapping_response() -> serde_json::Value {
    json!({
        "data": {
            "learnMapping": {
                "id": "map-1",
                "kind": "REPO_PATH",
                "pattern": "/path",
                "gryzzlyProjectId": "p1",
                "gryzzlyProjectName": "Project One",
                "isEnabled": true
            }
        }
    })
}

/// Pull the `variables` object out of the single request the wiremock server
/// received, asserting exactly one request was made to `LearnMapping`.
async fn learn_mapping_variables_sent(server: &MockServer) -> serde_json::Value {
    let requests = server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    let req = requests
        .iter()
        .find(|r| {
            r.body_json::<serde_json::Value>()
                .ok()
                .and_then(|b| b.get("operationName").and_then(|v| v.as_str()).map(String::from))
                .as_deref()
                == Some("LearnMapping")
        })
        .expect("LearnMapping was never called");
    let body: serde_json::Value = req.body_json().expect("request body is valid JSON");
    body["variables"].clone()
}

#[tokio::test]
async fn map_add_repo_sends_repo_path_kind_and_pattern() {
    let server = mock_graphql(learn_mapping_response()).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args([
            "--api-url", &url, "map", "add", "--repo", "/path", "--project", "p1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mapping saved"))
        .stdout(predicate::str::contains("p1"));

    let vars = learn_mapping_variables_sent(&server).await;
    assert_eq!(vars["kind"], json!("REPO_PATH"));
    assert_eq!(vars["pattern"], json!("/path"));
    assert_eq!(vars["gryzzlyProjectId"], json!("p1"));
    assert!(
        vars.get("branchPattern").is_none() || vars["branchPattern"].is_null(),
        "branchPattern must not be set for a bare --repo selector, got: {vars:?}"
    );
}

#[tokio::test]
async fn map_add_repo_with_branch_sends_branch_kind_and_branch_pattern() {
    let server = mock_graphql(json!({
        "data": {
            "learnMapping": {
                "id": "map-2",
                "kind": "BRANCH",
                "pattern": "/path",
                "gryzzlyProjectId": "p1",
                "gryzzlyProjectName": "Project One",
                "isEnabled": true
            }
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args([
            "--api-url", &url, "map", "add", "--repo", "/path", "--branch", "main", "--project",
            "p1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mapping saved"));

    let vars = learn_mapping_variables_sent(&server).await;
    assert_eq!(vars["kind"], json!("BRANCH"));
    assert_eq!(vars["pattern"], json!("/path"));
    assert_eq!(vars["branchPattern"], json!("main"));
    assert_eq!(vars["gryzzlyProjectId"], json!("p1"));
}

#[tokio::test]
async fn map_add_meeting_subject_sends_meeting_subject_kind() {
    let server = mock_graphql(json!({
        "data": {
            "learnMapping": {
                "id": "map-3",
                "kind": "MEETING_SUBJECT",
                "pattern": "standup",
                "gryzzlyProjectId": "p1",
                "gryzzlyProjectName": "Project One",
                "isEnabled": true
            }
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args([
            "--api-url",
            &url,
            "map",
            "add",
            "--meeting-subject",
            "standup",
            "--project",
            "p1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mapping saved"));

    let vars = learn_mapping_variables_sent(&server).await;
    assert_eq!(vars["kind"], json!("MEETING_SUBJECT"));
    assert_eq!(vars["pattern"], json!("standup"));
    assert_eq!(vars["gryzzlyProjectId"], json!("p1"));
}

#[tokio::test]
async fn map_add_meeting_organizer_sends_meeting_organizer_kind() {
    let server = mock_graphql(json!({
        "data": {
            "learnMapping": {
                "id": "map-4",
                "kind": "MEETING_ORGANIZER",
                "pattern": "alice@example.com",
                "gryzzlyProjectId": "p1",
                "gryzzlyProjectName": "Project One",
                "isEnabled": true
            }
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args([
            "--api-url",
            &url,
            "map",
            "add",
            "--meeting-organizer",
            "alice@example.com",
            "--project",
            "p1",
        ])
        .assert()
        .success();

    let vars = learn_mapping_variables_sent(&server).await;
    assert_eq!(vars["kind"], json!("MEETING_ORGANIZER"));
    assert_eq!(vars["pattern"], json!("alice@example.com"));
}

#[tokio::test]
async fn map_add_internal_project_sends_internal_project_kind() {
    let server = mock_graphql(json!({
        "data": {
            "learnMapping": {
                "id": "map-5",
                "kind": "INTERNAL_PROJECT",
                "pattern": "internal-ops",
                "gryzzlyProjectId": "p1",
                "gryzzlyProjectName": "Project One",
                "isEnabled": true
            }
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args([
            "--api-url",
            &url,
            "map",
            "add",
            "--internal-project",
            "internal-ops",
            "--project",
            "p1",
        ])
        .assert()
        .success();

    let vars = learn_mapping_variables_sent(&server).await;
    assert_eq!(vars["kind"], json!("INTERNAL_PROJECT"));
    assert_eq!(vars["pattern"], json!("internal-ops"));
}

// ---------------------------------------------------------------------------
// Precondition: exactly one selector is required. With none provided, the
// command must fail fast (exit code 4 = PreconditionFailed) WITHOUT ever
// calling the API — no wasted/erroneous network request on bad input.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn map_add_without_a_selector_fails_precondition_and_makes_no_request() {
    let server = MockServer::start().await;
    // No mocks mounted: any request that reaches the server would 404,
    // and we additionally assert `received_requests()` is empty below.
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "map", "add", "--project", "p1"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains(
            "provide one of --repo / --meeting-subject / --meeting-organizer / --internal-project",
        ));

    let requests = server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(
        requests.is_empty(),
        "`map add` must reject a missing selector before calling the API, but sent: {requests:?}"
    );
}

#[tokio::test]
async fn map_list_renders_rules_with_project_and_pattern() {
    let server = mock_graphql(json!({
        "data": {
            "signalMappings": [
                {
                    "id": "map-1",
                    "kind": "BRANCH",
                    "pattern": "/repos/aggregated_plan",
                    "branchPattern": "feat/*",
                    "gryzzlyProjectId": "PROJ-A",
                    "gryzzlyProjectName": "Project Alpha",
                    "isEnabled": true
                },
                {
                    "id": "map-2",
                    "kind": "MEETING_SUBJECT",
                    "pattern": "standup",
                    "branchPattern": null,
                    "gryzzlyProjectId": "PROJ-B",
                    "gryzzlyProjectName": null,
                    "isEnabled": true
                }
            ]
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "map", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("BRANCH"))
        .stdout(predicate::str::contains("/repos/aggregated_plan"))
        .stdout(predicate::str::contains("feat/*"))
        .stdout(predicate::str::contains("PROJ-A"))
        .stdout(predicate::str::contains("Project Alpha"))
        .stdout(predicate::str::contains("MEETING_SUBJECT"))
        .stdout(predicate::str::contains("standup"))
        .stdout(predicate::str::contains("PROJ-B"));
}

#[tokio::test]
async fn map_list_json_emits_raw_signal_mappings_payload() {
    let server = mock_graphql(json!({
        "data": {
            "signalMappings": [
                {
                    "id": "map-1",
                    "kind": "REPO_PATH",
                    "pattern": "/path",
                    "branchPattern": null,
                    "gryzzlyProjectId": "PROJ-A",
                    "gryzzlyProjectName": "Project Alpha",
                    "isEnabled": true
                }
            ]
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "--json", "map", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("signalMappings"))
        .stdout(predicate::str::contains("PROJ-A"))
        .stdout(predicate::str::contains("\"kind\":\"REPO_PATH\""));
}
