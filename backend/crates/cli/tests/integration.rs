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
async fn done_completes_current_task_and_stops_timer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CurrentActivity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "currentActivity": {
                    "id": "00000000-0000-0000-0000-000000000010",
                    "taskId": "00000000-0000-0000-0000-000000000001",
                    "startTime": "2026-04-08T09:00:00Z",
                    "halfDay": "MORNING",
                    "date": "2026-04-08",
                    "task": { "id": "00000000-0000-0000-0000-000000000001", "title": "Auth migration" }
                }
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CompleteTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "completeTask": {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Auth migration",
                    "sourceId": "AP-1234",
                    "status": "DONE"
                }
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("StopActivity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "stopActivity": {
                    "id": "00000000-0000-0000-0000-000000000010",
                    "taskId": "00000000-0000-0000-0000-000000000001",
                    "startTime": "2026-04-08T09:00:00Z",
                    "endTime": "2026-04-08T10:47:00Z",
                    "halfDay": "MORNING",
                    "date": "2026-04-08",
                    "durationMinutes": 107,
                    "task": { "id": "00000000-0000-0000-0000-000000000001", "title": "Auth migration" }
                }
            }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "done"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AP-1234"))
        .stdout(predicate::str::contains("done"))
        .stdout(predicate::str::contains("1h 47m"));
}

#[tokio::test]
async fn done_with_keep_running_does_not_stop_timer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CurrentActivity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "currentActivity": {
                    "id": "00000000-0000-0000-0000-000000000010",
                    "taskId": "00000000-0000-0000-0000-000000000001",
                    "startTime": "2026-04-08T09:00:00Z",
                    "halfDay": "MORNING",
                    "date": "2026-04-08",
                    "task": { "id": "00000000-0000-0000-0000-000000000001", "title": "Auth migration" }
                }
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CompleteTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "completeTask": {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Auth migration",
                    "sourceId": "AP-1234",
                    "status": "DONE"
                }
            }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "done", "--keep-running"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AP-1234"))
        .stdout(predicate::str::contains("done"))
        .stdout(predicate::str::contains("1h 47m").not());
}

#[tokio::test]
async fn triage_sets_tracking_state() {
    let server = mock_graphql(json!({
        "data": {
            "setTrackingState": {
                "id": "00000000-0000-0000-0000-000000000001",
                "title": "Auth migration",
                "sourceId": "AP-1234",
                "trackingState": "FOLLOWED"
            }
        }
    })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "triage", "followed", "00000000-0000-0000-0000-000000000001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AP-1234"))
        .stdout(predicate::str::contains("FOLLOWED"));
}

#[tokio::test]
async fn status_updates_currently_tracked_task() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CurrentActivity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "currentActivity": {
                    "id": "00000000-0000-0000-0000-000000000010",
                    "taskId": "00000000-0000-0000-0000-000000000001",
                    "startTime": "2026-04-08T09:00:00Z",
                    "halfDay": "MORNING",
                    "date": "2026-04-08",
                    "task": { "id": "00000000-0000-0000-0000-000000000001", "title": "Auth migration" }
                }
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateTaskStatus"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "updateTask": {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Auth migration",
                    "sourceId": "AP-1234",
                    "status": "IN_PROGRESS"
                }
            }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "status", "in_progress"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AP-1234"))
        .stdout(predicate::str::contains("IN_PROGRESS"));
}

#[tokio::test]
async fn note_appends_to_current_activity_task() {
    let server = MockServer::start().await;
    // First call: currentActivity returns a slot with a task
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CurrentActivity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "currentActivity": {
                    "id": "00000000-0000-0000-0000-000000000010",
                    "taskId": "00000000-0000-0000-0000-000000000001",
                    "startTime": "2026-04-08T09:00:00Z",
                    "halfDay": "MORNING",
                    "date": "2026-04-08",
                    "task": { "id": "00000000-0000-0000-0000-000000000001", "title": "Auth migration" }
                }
            }
        })))
        .mount(&server)
        .await;
    // Second call: appendTaskNotes
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("AppendTaskNotes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "appendTaskNotes": {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Auth migration",
                    "sourceId": "AP-1234",
                    "notes": "earlier line\n\nlock contention spikes at 30s"
                }
            }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "note", "lock", "contention", "spikes", "at", "30s"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AP-1234"))
        .stdout(predicate::str::contains("note appended"));
}

#[tokio::test]
async fn note_without_current_activity_exits_4() {
    let server = mock_graphql(json!({ "data": { "currentActivity": null } })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "note", "anything"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("no worklog is currently running"));
}

#[tokio::test]
async fn stop_prints_duration() {
    let server = mock_graphql(json!({
        "data": {
            "stopActivity": {
                "id": "00000000-0000-0000-0000-000000000010",
                "taskId": "00000000-0000-0000-0000-000000000001",
                "startTime": "2026-04-08T09:00:00Z",
                "endTime": "2026-04-08T10:47:00Z",
                "halfDay": "MORNING",
                "date": "2026-04-08",
                "durationMinutes": 107,
                "task": { "id": "00000000-0000-0000-0000-000000000001", "title": "Auth migration" }
            }
        }
    })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "stop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Auth migration"))
        .stdout(predicate::str::contains("1h 47m"));
}

#[tokio::test]
async fn stop_with_no_running_activity() {
    let server = mock_graphql(json!({ "data": { "stopActivity": null } })).await;
    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "stop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no activity"));
}

#[tokio::test]
async fn dash_prints_summary_sections() {
    let server = mock_graphql(json!({
        "data": {
            "dailyDashboard": {
                "date": "2026-04-08",
                "tasks": [
                    { "id": "00000000-0000-0000-0000-000000000001", "title": "Auth migration", "sourceId": "AP-1234", "status": "IN_PROGRESS", "urgency": "HIGH", "impact": "HIGH" }
                ],
                "meetings": [
                    { "id": "00000000-0000-0000-0000-000000000020", "title": "Standup", "startTime": "2026-04-08T09:30:00Z", "endTime": "2026-04-08T09:45:00Z" }
                ],
                "alerts": [
                    { "id": "00000000-0000-0000-0000-000000000030", "alertType": "DEADLINE", "severity": "WARNING", "message": "AP-1234 due in 3 days" }
                ]
            }
        }
    })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "dash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Auth migration"))
        .stdout(predicate::str::contains("Standup"))
        .stdout(predicate::str::contains("due in 3 days"));
}

#[tokio::test]
async fn show_prints_task_detail() {
    let server = mock_graphql(json!({
        "data": {
            "task": {
                "id": "00000000-0000-0000-0000-000000000001",
                "title": "Auth migration",
                "description": "Migrate auth middleware to new compliance model.",
                "notes": "Saw lock contention at 30s.",
                "sourceId": "AP-1234",
                "status": "IN_PROGRESS",
                "urgency": "HIGH",
                "impact": "HIGH",
                "quadrant": "URGENT_IMPORTANT",
                "trackingState": "FOLLOWED",
                "deadline": "2026-04-15",
                "plannedStart": null,
                "plannedEnd": null,
                "estimatedHours": 8.0
            }
        }
    })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "show", "00000000-0000-0000-0000-000000000001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AP-1234"))
        .stdout(predicate::str::contains("Auth migration"))
        .stdout(predicate::str::contains("URGENT_IMPORTANT"))
        .stdout(predicate::str::contains("Saw lock contention"));
}

#[tokio::test]
async fn ls_prints_a_table_of_tasks() {
    let server = mock_graphql(json!({
        "data": {
            "tasks": {
                "totalCount": 2,
                "edges": [
                    {
                        "node": {
                            "id": "00000000-0000-0000-0000-000000000001",
                            "title": "Auth migration",
                            "sourceId": "AP-1234",
                            "status": "IN_PROGRESS",
                            "urgency": "HIGH",
                            "impact": "HIGH",
                            "trackingState": "FOLLOWED",
                            "deadline": "2026-04-15"
                        }
                    },
                    {
                        "node": {
                            "id": "00000000-0000-0000-0000-000000000002",
                            "title": "DB backup",
                            "sourceId": null,
                            "status": "TODO",
                            "urgency": "LOW",
                            "impact": "MEDIUM",
                            "trackingState": "FOLLOWED",
                            "deadline": null
                        }
                    }
                ]
            }
        }
    })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AP-1234"))
        .stdout(predicate::str::contains("Auth migration"))
        .stdout(predicate::str::contains("DB backup"))
        .stdout(predicate::str::contains("2 task"));
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
