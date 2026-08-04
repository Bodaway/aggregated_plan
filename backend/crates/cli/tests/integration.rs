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

/// Canonical hydrated task title. `resolve_task` now issues `GetTask` to
/// hydrate a UUID token (or the current-activity pointer) before proceeding,
/// so flows that resolve a UUID surface this real title instead of an empty one.
const ACTIVE_TASK_TITLE: &str = "Présentation Similarity IA — équipe de suivi";

/// Full `GetTask` response body for the canonical active task
/// (id `00000000-0000-0000-0000-000000000001`). Selects every field
/// `graphql/get_task.graphql` asks for; nullable fields are left null.
fn get_task_body() -> serde_json::Value {
    json!({
        "data": {
            "task": {
                "id": "00000000-0000-0000-0000-000000000001",
                "title": ACTIVE_TASK_TITLE,
                "description": null,
                "notes": null,
                "sourceId": "AP-1234",
                "status": "IN_PROGRESS",
                "urgency": "HIGH",
                "impact": "HIGH",
                "quadrant": "URGENT_IMPORTANT",
                "trackingState": "FOLLOWED",
                "deadline": null,
                "plannedStart": null,
                "plannedEnd": null,
                "estimatedHours": null
            }
        }
    })
}

/// Mount a `GetTask` mock on `server` returning the canonical hydrated task.
async fn mount_get_task(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(get_task_body()))
        .mount(server)
        .await;
}

#[tokio::test]
async fn current_with_running_activity_prints_one_line() {
    // `current` reads aplan.active_task_id via GetConfiguration, then hydrates
    // that UUID through GetTask (the pointer is resolved like any other token).
    // The hydrated title is printed as "▶ tracking: <title>".
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": "00000000-0000-0000-0000-000000000001"
                }
            }
        })))
        .mount(&server)
        .await;
    mount_get_task(&server).await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "▶ tracking: {}",
            ACTIVE_TASK_TITLE
        )));
}

#[tokio::test]
async fn current_with_no_activity_prints_placeholder() {
    // GetConfiguration returns an empty aplan.active_task_id → "no task being tracked"
    let server = mock_graphql(json!({
        "data": {
            "configuration": {
                "aplan.active_task_id": ""
            }
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no task being tracked"));
}

#[tokio::test]
async fn current_with_json_flag_emits_raw_data_block() {
    // GetConfiguration returns empty pointer → JSON {"currentActivity":null}
    let server = mock_graphql(json!({
        "data": {
            "configuration": {}
        }
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
async fn current_with_json_flag_and_stale_pointer_emits_null() {
    // The pointer names a task that GetTask can no longer find (task: null).
    // resolve_task fails with NotFound; in JSON mode `current` swallows the
    // resolve error and still succeeds, reporting {"currentActivity":null}.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": "00000000-0000-0000-0000-000000000001"
                }
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "task": null }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "--json", "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"currentActivity\":null"));
}

#[tokio::test]
async fn done_completes_current_task_and_flushes_worklog() {
    // `done` with no task token reads active_task_id from GetConfiguration,
    // completes via CompleteTask, checks the pointer again via GetConfiguration,
    // then flushes via FlushWorklogTime and clears the pointer via UpdateConfiguration.
    let server = MockServer::start().await;
    // Both GetConfiguration calls return the same active id (pointer at AP-1234 task).
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": "00000000-0000-0000-0000-000000000001"
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
    // FlushWorklogTime and UpdateConfiguration can return minimal payloads.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
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
        // No more timer-duration text in the new flow
        .stdout(predicate::str::contains("1h 47m").not());
}

#[tokio::test]
async fn done_with_keep_running_does_not_flush_worklog() {
    // --keep-running: pointer is NOT cleared and FlushWorklogTime is NOT called.
    // The test registers no FlushWorklogTime mock; wiremock would return 404
    // if the CLI issued that call, making the test fail.
    let server = MockServer::start().await;
    // GetConfiguration (active_task_id check — called once since there's a task token).
    // done is called without a task token here, so it reads the active id first,
    // then re-reads after CompleteTask to decide whether to flush.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": "00000000-0000-0000-0000-000000000001"
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
        .stdout(predicate::str::contains("done"));
}

#[tokio::test]
async fn triage_sets_tracking_state() {
    // triage resolves the UUID token via GetTask, then issues SetTrackingState.
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("SetTrackingState"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "setTrackingState": {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Auth migration",
                    "sourceId": "AP-1234",
                    "trackingState": "FOLLOWED"
                }
            }
        })))
        .mount(&server)
        .await;
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
    // resolve_task (implicit) reads aplan.active_task_id from GetConfiguration,
    // then hydrates that UUID via GetTask before issuing the status mutation.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": "00000000-0000-0000-0000-000000000001"
                }
            }
        })))
        .mount(&server)
        .await;
    mount_get_task(&server).await;
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
async fn note_appends_to_active_task() {
    let server = MockServer::start().await;
    // resolve_task (implicit) reads aplan.active_task_id from GetConfiguration,
    // then hydrates that UUID via GetTask before appending notes.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": "00000000-0000-0000-0000-000000000001"
                }
            }
        })))
        .mount(&server)
        .await;
    mount_get_task(&server).await;
    // Third call: appendTaskNotes
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
async fn note_without_active_task_exits_4() {
    // GetConfiguration returns an empty map (no aplan.active_task_id) →
    // resolve_task fails with LookupError::NoCurrentActivity → exit code 4.
    let server = mock_graphql(json!({ "data": { "configuration": {} } })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "note", "anything"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("no worklog is currently running"));
}

#[tokio::test]
async fn stop_flushes_worklog_and_clears_pointer() {
    // stop: reads active_task_id (GetConfiguration), calls FlushWorklogTime,
    // then clears the pointer (UpdateConfiguration).
    // Human output: "⏹ stopped — worklog time flushed, tracking cleared"
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": "00000000-0000-0000-0000-000000000001"
                }
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "stop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("worklog time flushed"))
        .stdout(predicate::str::contains("tracking cleared"));
}

#[tokio::test]
async fn stop_with_no_running_activity() {
    // GetConfiguration returns empty pointer → "(no task was being tracked)".
    // UpdateConfiguration is still called to clear (idempotent).
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": ""
                }
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "stop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no task was being tracked"));
}

#[tokio::test]
async fn alerts_prints_unresolved_by_default() {
    let server = mock_graphql(json!({
        "data": {
            "alerts": {
                "totalCount": 1,
                "edges": [
                    {
                        "node": {
                            "id": "00000000-0000-0000-0000-000000000030",
                            "alertType": "DEADLINE",
                            "severity": "WARNING",
                            "message": "AP-1234 due in 3 days",
                            "date": "2026-04-08",
                            "resolved": false,
                            "createdAt": "2026-04-08T08:00:00Z"
                        }
                    }
                ]
            }
        }
    })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "alerts"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AP-1234"))
        .stdout(predicate::str::contains("1 alerts"));
}

#[tokio::test]
async fn journal_prints_slots_and_total() {
    let server = mock_graphql(json!({
        "data": {
            "activityJournal": [
                {
                    "id": "00000000-0000-0000-0000-000000000010",
                    "taskId": "00000000-0000-0000-0000-000000000001",
                    "startTime": "2026-04-08T09:00:00Z",
                    "endTime": "2026-04-08T10:30:00Z",
                    "halfDay": "MORNING",
                    "durationMinutes": 90,
                    "task": { "id": "00000000-0000-0000-0000-000000000001", "title": "Auth migration" }
                }
            ]
        }
    })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "journal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Auth migration"))
        .stdout(predicate::str::contains("1h 30m"))
        .stdout(predicate::str::contains("total"));
}

#[tokio::test]
async fn matrix_prints_four_quadrants() {
    let server = mock_graphql(json!({
        "data": {
            "priorityMatrix": {
                "urgentImportant": [
                    { "id": "00000000-0000-0000-0000-000000000001", "title": "Auth migration", "sourceId": "AP-1234", "urgency": "HIGH", "impact": "HIGH" }
                ],
                "important": [],
                "urgent":    [],
                "neither":   []
            }
        }
    })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "matrix"])
        .assert()
        .success()
        .stdout(predicate::str::contains("URGENT + IMPORTANT"))
        .stdout(predicate::str::contains("Auth migration"));
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
async fn rm_deletes_a_task_by_uuid() {
    // rm resolves the UUID token via GetTask, then issues DeleteTask.
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("DeleteTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "deleteTask": true }
        })))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args([
            "--api-url",
            &url,
            "rm",
            "00000000-0000-0000-0000-000000000001",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("deleted"));
}

#[tokio::test]
async fn new_creates_personal_task() {
    let server = mock_graphql(json!({
        "data": {
            "createTask": {
                "id": "00000000-0000-0000-0000-000000000001",
                "title": "Write postmortem",
                "sourceId": null,
                "status": "TODO",
                "urgency": "MEDIUM",
                "impact": "MEDIUM"
            }
        }
    })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "new", "Write postmortem"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created"))
        .stdout(predicate::str::contains("Write postmortem"));
}

#[tokio::test]
async fn start_with_uuid_token_starts_activity() {
    // UUID token: resolve_task hydrates the task via GetTask, yielding the real
    // title. start then reads the previous pointer (GetConfiguration), finds
    // nothing, and writes the new pointer via UpdateConfiguration (twice: id +
    // since). Human output: "▶ tracking: <title>".
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {}
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .mount(&server)
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
        .stdout(predicate::str::contains("▶ tracking"));
}

#[tokio::test]
async fn sync_prints_source_statuses() {
    let server = mock_graphql(json!({
        "data": {
            "forceSync": [
                {
                    "source": "JIRA",
                    "status": "SUCCESS",
                    "lastSyncAt": "2026-04-08T09:00:00Z",
                    "errorMessage": null
                }
            ]
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "sync", "--source", "jira"])
        .assert()
        .success()
        .stdout(predicate::str::contains("JIRA"))
        .stdout(predicate::str::contains("SUCCESS"));
}

#[tokio::test]
async fn resolve_marks_alert_resolved() {
    let server = mock_graphql(json!({
        "data": {
            "resolveAlert": {
                "id": "00000000-0000-0000-0000-000000000030",
                "alertType": "DEADLINE",
                "severity": "WARNING",
                "message": "AP-1234 due in 3 days",
                "resolved": true
            }
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args([
            "--api-url",
            &url,
            "resolve",
            "00000000-0000-0000-0000-000000000030",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("resolved"));
}

#[tokio::test]
async fn config_get_prints_all_keys() {
    let server = mock_graphql(json!({
        "data": {
            "configuration": {
                "general.working_hours": "8",
                "jira.url": "https://example.atlassian.net"
            }
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "config", "get"])
        .assert()
        .success()
        .stdout(predicate::str::contains("general.working_hours"))
        .stdout(predicate::str::contains("jira.url"));
}

#[tokio::test]
async fn config_set_sets_a_key() {
    let server = mock_graphql(json!({ "data": { "updateConfiguration": true } })).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args([
            "--api-url",
            &url,
            "config",
            "set",
            "general.working_hours",
            "7",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("general.working_hours = 7"));
}

#[tokio::test]
async fn priority_sets_urgency_and_impact() {
    // priority resolves the UUID token via GetTask, then issues UpdatePriority.
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdatePriority"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "updatePriority": {
                    "id": "00000000-0000-0000-0000-000000000001",
                    "title": "Auth migration",
                    "sourceId": "AP-1234",
                    "urgency": "HIGH",
                    "impact": "CRITICAL",
                    "urgencyManual": true
                }
            }
        })))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args([
            "--api-url",
            &url,
            "priority",
            "00000000-0000-0000-0000-000000000001",
            "--urgency",
            "high",
            "--impact",
            "critical",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("AP-1234"))
        .stdout(predicate::str::contains("HIGH"))
        .stdout(predicate::str::contains("CRITICAL"));
}

// ---------------------------------------------------------------------------
// Task 10 — start / log / stop pointer lifecycle (per-op assertions)
//
// The wiremock server cannot model stateful config (set-then-get returning
// the new value) because all requests within one MockServer share the same
// in-process mock registry.  This test therefore asserts each command in
// isolation against its own dedicated server, verifying the operations each
// command issues rather than a single shared stateful backend.
// ---------------------------------------------------------------------------

/// `aplan start <uuid>` hydrates the token via GetTask, issues GetConfiguration
/// (prior pointer check), then UpdateConfiguration (sets aplan.active_task_id
/// and aplan.active_since).
#[tokio::test]
async fn start_log_stop_pointer_lifecycle_start_sets_pointer() {
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "configuration": {} }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .mount(&server)
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
        .stdout(predicate::str::contains("▶ tracking"));
}

/// `aplan current --json` reports the active task under `.currentActivity.task`
/// when GetConfiguration returns a non-empty aplan.active_task_id. The pointer
/// is hydrated through GetTask, so the payload carries the real title, not an
/// empty one.
#[tokio::test]
async fn start_log_stop_pointer_lifecycle_current_reports_task() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    // GetConfiguration returns the id we just "started"
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": task_id
                }
            }
        })))
        .mount(&server)
        .await;
    mount_get_task(&server).await;

    let url = format!("{}/graphql", server.uri());
    // UUID pointer → resolve_task hydrates via GetTask; both id and title present.
    aplan()
        .args(["--api-url", &url, "--json", "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains(task_id))
        .stdout(predicate::str::contains(ACTIVE_TASK_TITLE));
}

/// `aplan log "did a thing"` with a UUID token hydrates it via GetTask, then
/// issues AddWorklogEntry.
#[tokio::test]
async fn start_log_stop_pointer_lifecycle_log_issues_add_worklog_entry() {
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("AddWorklogEntry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "addWorklogEntry": {
                    "id": "00000000-0000-0000-0000-000000000020",
                    "taskId": "00000000-0000-0000-0000-000000000001",
                    "loggedAt": "2026-04-08T10:00:00Z"
                }
            }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args([
            "--api-url",
            &url,
            "log",
            "--task",
            "00000000-0000-0000-0000-000000000001",
            "did a thing",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("worklog entry added"));
}

/// `aplan start <uuid>` when the SAME task is already active must flush the
/// previous worklog before repointing (so entries since the last watermark are
/// not lost when `aplan.active_since` is reset).
#[tokio::test]
async fn start_on_already_active_same_task_flushes_previous() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    // resolve_task hydrates the UUID token via GetTask before start proceeds.
    mount_get_task(&server).await;
    // GetConfiguration returns the same task id we are about to re-start.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": task_id
                }
            }
        })))
        .mount(&server)
        .await;
    // FlushWorklogTime MUST be called exactly once — this is the regression gate.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .expect(1)
        .mount(&server)
        .await;
    // UpdateConfiguration is called to set the new pointer + active_since.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "start", task_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("▶ tracking"));
    // wiremock verifies the .expect(1) on FlushWorklogTime when `server` drops.
}

/// `aplan stop --json` issues FlushWorklogTime + UpdateConfiguration and returns
/// `{"stopped": <id>}` in JSON mode; subsequent current reports null.
#[tokio::test]
async fn start_log_stop_pointer_lifecycle_stop_clears_pointer() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "configuration": {
                    "aplan.active_task_id": task_id
                }
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    // stop --json → {"stopped": "<task_id>"}
    aplan()
        .args(["--api-url", &url, "--json", "stop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"stopped\""))
        .stdout(predicate::str::contains(task_id));
}

// ---------------------------------------------------------------------------
// Task 9 — `--session`, resolution order, `session`/`sessions` commands
// ---------------------------------------------------------------------------

/// Session-aware flows must never inherit the developer's own session id: the suite
/// runs inside a Claude Code session, where `CLAUDE_CODE_SESSION_ID` is exported into
/// every command. Without this the test exercises a different branch than it claims.
fn aplan_no_session() -> Command {
    let mut cmd = Command::cargo_bin("aplan").unwrap();
    cmd.env_remove("CLAUDE_CODE_SESSION_ID");
    cmd
}

/// `claudeSession(id:)` response body. The field is `claudeSession`, not `session` —
/// that name is already the Microsoft-OAuth status query the frontend's auth gate
/// consumes.
fn session_body(mode: &str, task_id: Option<&str>) -> serde_json::Value {
    json!({
        "data": {
            "claudeSession": {
                "id": "s1",
                "taskId": task_id,
                "mode": mode,
                "label": "/home/mbt/appfactory/aggregated_plan",
                "startedAt": "2026-08-04T09:00:00+00:00",
                "lastSeenAt": "2026-08-04T09:00:00+00:00",
                "lastFlushAt": null,
                "endedAt": null
            }
        }
    })
}

#[tokio::test]
async fn log_targets_the_sessions_task_not_the_global_pointer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body(
            "TRACKING",
            Some("00000000-0000-0000-0000-000000000001"),
        )))
        .mount(&server)
        .await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("AddWorklogEntry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "addWorklogEntry": {
                "id": "e1", "taskId": "00000000-0000-0000-0000-000000000001",
                "loggedAt": "2026-08-04T10:00:00+00:00", "sessionId": "s1" } }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "log", "fait"])
        .assert()
        .success()
        .stdout(predicate::str::contains("worklog entry added"));
}

#[tokio::test]
async fn log_refuses_exit_4_when_the_session_is_not_tracked() {
    // The bug this feature exists to kill: an opted-out session must refuse, not
    // fall back onto the human's pointer.
    let server = mock_graphql(session_body("OFF", None)).await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "log", "fait"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("not tracked"));
}

#[tokio::test]
async fn log_refuses_exit_4_when_the_session_has_no_task() {
    let server = mock_graphql(session_body("TRACKING", None)).await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "log", "fait"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("no task"));
}

#[tokio::test]
async fn an_explicit_task_wins_over_the_session() {
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("AddWorklogEntry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "addWorklogEntry": {
                "id": "e1", "taskId": "00000000-0000-0000-0000-000000000001",
                "loggedAt": "2026-08-04T10:00:00+00:00", "sessionId": null } }
        })))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    // No `Session` mock is mounted: resolving one would be a bug, and the missing
    // stub makes that visible instead of silent.
    aplan_no_session()
        .args([
            "--api-url", &url, "--session", "s1", "log", "fait",
            "--task", "00000000-0000-0000-0000-000000000001",
        ])
        .assert()
        .success();
}

#[tokio::test]
async fn without_a_session_the_global_pointer_still_answers() {
    // The human, working by hand. Unchanged behaviour.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "configuration": {
                "aplan.active_task_id": "00000000-0000-0000-0000-000000000001" } }
        })))
        .mount(&server)
        .await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("AddWorklogEntry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "addWorklogEntry": {
                "id": "e1", "taskId": "00000000-0000-0000-0000-000000000001",
                "loggedAt": "2026-08-04T10:00:00+00:00", "sessionId": null } }
        })))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "log", "fait"])
        .assert()
        .success();
}

#[tokio::test]
async fn the_session_id_is_picked_up_from_the_environment() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body("OFF", None)))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    // Refusing on an OFF session proves the env var reached the resolver: a CLI
    // that ignored it would have fallen through to the global pointer and asked
    // for `GetConfiguration`, which is not mocked here.
    aplan()
        .env("CLAUDE_CODE_SESSION_ID", "s1")
        .args(["--api-url", &url, "log", "fait"])
        .assert()
        .code(4);
}

#[tokio::test]
async fn sessions_lists_the_open_ones() {
    let server = mock_graphql(json!({
        "data": { "openClaudeSessions": [
            { "id": "s1", "taskId": "00000000-0000-0000-0000-000000000001",
              "task": { "id": "00000000-0000-0000-0000-000000000001", "title": "Saft cadrage" },
              "mode": "TRACKING", "label": "/home/mbt/x",
              "startedAt": "2026-08-04T09:00:00+00:00",
              "lastSeenAt": "2026-08-04T10:30:00+00:00",
              "lastFlushAt": null, "endedAt": null }
        ] }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "sessions"])
        .assert()
        .success()
        .stdout(predicate::str::contains("s1"))
        .stdout(predicate::str::contains("Saft cadrage"));
}

#[tokio::test]
async fn session_off_persists_the_decision() {
    let server = mock_graphql(json!({
        "data": { "setSessionMode": {
            "id": "s1", "taskId": null, "mode": "OFF", "label": null,
            "startedAt": "2026-08-04T09:00:00+00:00",
            "lastSeenAt": "2026-08-04T09:00:00+00:00",
            "lastFlushAt": null, "endedAt": null } }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "session", "off", "--session", "s1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not tracking"));
}
