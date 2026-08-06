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

/// The harness exports `CLAUDE_CODE_SESSION_ID` into every Bash call, including
/// the one running this suite. Stripped here, once, so every test is env-clean
/// by construction instead of by discipline: a test that does not deliberately
/// set the variable can never silently exercise the session branch and pass
/// for the wrong reason. A test that wants the opposite sets it explicitly —
/// see `the_session_id_is_picked_up_from_the_environment`.
fn aplan() -> Command {
    let mut cmd = Command::cargo_bin("aplan").unwrap();
    cmd.env_remove("CLAUDE_CODE_SESSION_ID");
    cmd
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

/// `aplan()` is env-clean by construction now; this is kept as a readable alias
/// for the tests below that want to say explicitly "no session is bound here".
fn aplan_no_session() -> Command {
    aplan()
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

// ---------------------------------------------------------------------------
// C1 — an id that names no known session carries no decision to honour, so it
// must fall through to the global pointer exactly like an absent `--session`
// would (not refuse — that refusal is reserved for a session that *was* found
// and said no).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn log_with_an_unknown_session_id_falls_through_to_the_global_pointer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "claudeSession": null }
        })))
        .mount(&server)
        .await;
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
        .args(["--api-url", &url, "--session", "ghost", "log", "fait"])
        .assert()
        .success();
}

/// `done`'s `--session`-driven path is separate from `log`'s: `commands.rs`
/// treats the mere presence of the session flag as an explicit target before
/// it ever reaches `resolve_target`, so the fallthrough needs its own test or
/// it stays uncovered.
#[tokio::test]
async fn done_with_an_unknown_session_id_falls_through_to_the_global_pointer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "claudeSession": null }
        })))
        .mount(&server)
        .await;
    // Two GetConfiguration reads: the fallthrough resolution itself, then the
    // "was this the tracked task" check `done` makes before flushing.
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
        .and(wiremock::matchers::body_string_contains("CompleteTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "completeTask": {
                "id": "00000000-0000-0000-0000-000000000001",
                "title": "Auth migration", "sourceId": "AP-1234", "status": "DONE" } }
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

    aplan_no_session()
        .args(["--api-url", &url, "--session", "ghost", "done"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// C2 — `sessionId` goes on the wire only when the session level of
// resolution actually answered. `worklog_entries.session_id` is a real
// foreign key: sending an id with no row fails the insert.
// ---------------------------------------------------------------------------

/// Matches only when the request body carries no *string*-valued `sessionId`.
/// graphql-client serializes `session_id: None` as `"sessionId":null`, which
/// this matcher tolerates: the contract under test is "no session was
/// attributed to this write", not "the JSON key is physically absent".
struct NoSessionIdOnTheWire;

impl wiremock::Match for NoSessionIdOnTheWire {
    fn matches(&self, request: &wiremock::Request) -> bool {
        let body = String::from_utf8_lossy(&request.body);
        !body.contains(r#""sessionId":""#)
    }
}

#[tokio::test]
async fn an_explicit_task_wins_over_the_session() {
    // `--task` never touches the session — not for resolution (the missing
    // `Session` stub below makes that visible) and not for attribution: the
    // request must carry no `sessionId`, even though `--session s1` was also
    // passed. `NoSessionIdOnTheWire` makes a request that sends "s1" 404
    // instead of silently succeeding on the wrong contract.
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("AddWorklogEntry"))
        .and(NoSessionIdOnTheWire)
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

/// `CLAUDE_CODE_SESSION_ID=""` is how a hook running outside any Claude session
/// sets the var: present, but empty. It must behave exactly like an absent
/// `--session` — global pointer, no `sessionId` on the wire — not like a
/// session id of `""`.
#[tokio::test]
async fn an_empty_session_env_var_falls_back_to_the_global_pointer_with_no_session_id() {
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
        .and(NoSessionIdOnTheWire)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "addWorklogEntry": {
                "id": "e1", "taskId": "00000000-0000-0000-0000-000000000001",
                "loggedAt": "2026-08-04T10:00:00+00:00", "sessionId": null } }
        })))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    // No `Session` mock mounted: an empty env var resolving one would be a bug.
    aplan()
        .env("CLAUDE_CODE_SESSION_ID", "")
        .args(["--api-url", &url, "log", "fait"])
        .assert()
        .success();
}

/// The positive half of the C2 contract: without this, a CLI that simply never
/// sent `sessionId` at all would still pass the two negative tests above.
#[tokio::test]
async fn a_session_that_answers_puts_its_id_on_the_wire() {
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
        .and(wiremock::matchers::body_string_contains(r#""sessionId":"s1""#))
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

// ---------------------------------------------------------------------------
// I1 — `aplan session bind --json` must flush the previous task, same as the
// human-output path. `session_cmd.rs` returns from the `--json` branch before
// reaching the flush call, and `--json` is the path the hooks will use.
// ---------------------------------------------------------------------------

fn bind_session_body(task_id: &str, previous_task_id: &str) -> serde_json::Value {
    json!({
        "data": { "bindSession": {
            "session": { "id": "s1", "taskId": task_id, "mode": "TRACKING", "label": "/tmp/x" },
            "previousTaskId": previous_task_id
        } }
    })
}

#[tokio::test]
async fn session_bind_with_json_flushes_the_previous_task() {
    let new_task = "00000000-0000-0000-0000-000000000001";
    let previous_task = "00000000-0000-0000-0000-000000000002";
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("BindSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bind_session_body(new_task, previous_task)))
        .mount(&server)
        .await;
    // `.expect(1)` is the regression gate: the absence of this call must fail
    // the test rather than pass silently on the `--json` early return.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args([
            "--api-url", &url, "--session", "s1", "session", "bind", new_task, "--json",
        ])
        .assert()
        .success();
    // wiremock verifies .expect(1) on FlushWorklogTime when `server` drops.
}

/// The non-`--json` path already flushes; pinned green so a future refactor
/// cannot regress it while fixing the `--json` path above.
#[tokio::test]
async fn session_bind_flushes_the_previous_task_on_the_human_output_path() {
    let new_task = "00000000-0000-0000-0000-000000000001";
    let previous_task = "00000000-0000-0000-0000-000000000002";
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("BindSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bind_session_body(new_task, previous_task)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "session", "bind", new_task])
        .assert()
        .success()
        .stdout(predicate::str::contains("session s1"));
}

/// Task 6 — the flush a bind issues for the task it is leaving must carry
/// *this session's* id, not the global pointer's. A body matcher that only
/// accepts `"sessionId":"s1"` makes an unscoped flush fail the test instead
/// of passing quietly against a lenient stub.
#[tokio::test]
async fn session_bind_flushes_the_previous_task_against_its_own_session() {
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("BindSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "bindSession": {
                "session": { "id": "s1", "taskId": "00000000-0000-0000-0000-000000000001",
                             "mode": "TRACKING", "label": null, "endedAt": null },
                "previousTaskId": "00000000-0000-0000-0000-0000000000bb" } }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(wiremock::matchers::body_string_contains("\"sessionId\":\"s1\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "--session", "s1", "session", "bind",
               "00000000-0000-0000-0000-000000000001"])
        .assert()
        .success();
}

/// `aplan remember` deliberately does not follow the worklog verbs' refusal
/// rule. With no `--task`, no session, and nothing tracked, it must still
/// succeed — an unattached memory, not exit 4.
fn remember_body() -> serde_json::Value {
    json!({
        "data": { "remember": {
            "id": "00000000-0000-0000-0000-000000000040",
            "kind": "FACT",
            "title": "les tests passent",
            "body": null,
            "occurredAt": "2026-08-04T10:00:00+00:00",
            "recordedAt": "2026-08-04T10:00:00+00:00",
            "status": "PENDING",
            "source": "CLAUDE_SESSION",
            "projectId": null,
            "taskId": null,
            "stakeholders": [],
            "proposedSupersedes": null,
            "contradicts": null
        } }
    })
}

#[tokio::test]
async fn remember_with_nothing_tracked_creates_an_unattached_memory() {
    let server = mock_graphql(remember_body()).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "remember", "les tests passent"])
        .assert()
        .success();
}

#[tokio::test]
async fn remember_with_an_off_session_still_succeeds_unattached() {
    // Unlike `log`, an `OFF` session must not refuse `remember`: the
    // SessionStart hook already routes such a session to memories, not the
    // worklog, so the worklog's exit-4 rule does not apply here.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body("OFF", None)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Remember"))
        .respond_with(ResponseTemplate::new(200).set_body_json(remember_body()))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "--session", "s1", "remember", "les tests passent"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Task 9 — the data-loss path: `done` must flush against whoever was tracking
// the task (session or human), not gate the flush on the human's pointer
// alone. Before the fix, a session bound to task X running `done` on X while
// the human's pointer sat elsewhere — the steady state once sessions are in
// use — never flushed at all, silently dropping the session's worklog time.
// ---------------------------------------------------------------------------

/// A session's `done` must flush its own window (carrying its own session id)
/// even though the human's global pointer sits on an unrelated task — and
/// that unrelated pointer must be left alone, not blanked by a session's
/// `done`. `.expect(1)` / `.expect(0)` make either failure mode fail the test
/// instead of passing quietly: a flush that silently didn't happen, or a
/// human pointer that got cleared by someone else's `done`.
#[tokio::test]
async fn done_via_a_session_flushes_the_sessions_window_and_leaves_the_human_pointer_alone() {
    let task_id = "00000000-0000-0000-0000-000000000001"; // what the session is tracking
    let human_pointer = "00000000-0000-0000-0000-000000000002"; // unrelated, human's own task
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body(
            "TRACKING",
            Some(task_id),
        )))
        .mount(&server)
        .await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CompleteTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "completeTask": {
                "id": task_id, "title": "Auth migration",
                "sourceId": "AP-1234", "status": "DONE" } }
        })))
        .mount(&server)
        .await;
    // The "was this the tracked task" check reads the human's pointer, which
    // sits on a different task entirely — the steady state this task fixes.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "configuration": {
                "aplan.active_task_id": human_pointer } }
        })))
        .mount(&server)
        .await;
    // The regression gate: pre-fix, this request was never made at all — the
    // flush was gated on the human's pointer, which does not match here.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(wiremock::matchers::body_string_contains(r#""sessionId":"s1""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .expect(1)
        .mount(&server)
        .await;
    // The human's pointer must never be cleared by a session completing its
    // own task: clearing is keyed on the pointer alone, not on this flush.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .expect(0)
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "done"])
        .assert()
        .success();
    // wiremock verifies .expect(1) on FlushWorklogTime and .expect(0) on
    // UpdateConfiguration when `server` drops.
}

/// The human's own path, unchanged by the fix: no session, the pointer on the
/// task being completed (`via` is `GlobalPointer`, not `Session`). The flush
/// must still run, and it must carry no session id — reusing
/// `NoSessionIdOnTheWire` rather than a looser matcher that would accept
/// `"sessionId":"s1"` just as quietly.
#[tokio::test]
async fn done_without_a_session_still_flushes_the_humans_own_window_with_no_session_id() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "configuration": {
                "aplan.active_task_id": task_id } }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CompleteTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "completeTask": {
                "id": task_id, "title": "Auth migration",
                "sourceId": "AP-1234", "status": "DONE" } }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(NoSessionIdOnTheWire)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .expect(1)
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
        .success();
    // wiremock verifies .expect(1) on FlushWorklogTime when `server` drops.
}

// ---------------------------------------------------------------------------
// Task 9 left a third argument shape unfixed: an explicit TASK argument
// naming the very task a bound session is tracking. `resolve_target` gives
// an explicit target absolute precedence (`via == Task`), so the gate fell
// back to the human's pointer alone — and when that pointer sits elsewhere,
// no flush ran at all, silently losing the session's time on the task it
// just completed.
// ---------------------------------------------------------------------------

/// `aplan --session s1 done X` where `s1` is bound to `X` and the human's
/// pointer sits elsewhere. The explicit `X` still wins the resolution (`via`
/// is `Task`, not `Session`), but the flush must still carry `s1`'s id.
/// `.expect(1)` is the regression gate: pre-fix, this request was never made
/// at all — the gate consulted only the human's unrelated pointer.
#[tokio::test]
async fn done_with_task_naming_a_bound_sessions_own_task_still_flushes_that_session() {
    let task_id = "00000000-0000-0000-0000-000000000001"; // what s1 is tracking, and the explicit target
    let human_pointer = "00000000-0000-0000-0000-000000000002"; // unrelated, human's own task
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("Session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body(
            "TRACKING",
            Some(task_id),
        )))
        .mount(&server)
        .await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CompleteTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "completeTask": {
                "id": task_id, "title": "Auth migration",
                "sourceId": "AP-1234", "status": "DONE" } }
        })))
        .mount(&server)
        .await;
    // The gate's own pointer read sits on an unrelated task — the steady
    // state this test fixes.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "configuration": {
                "aplan.active_task_id": human_pointer } }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(wiremock::matchers::body_string_contains(r#""sessionId":"s1""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .expect(1)
        .mount(&server)
        .await;
    // The human's unrelated pointer must never be cleared by this `done`.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .expect(0)
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "done", task_id])
        .assert()
        .success();
    // wiremock verifies .expect(1) on FlushWorklogTime and .expect(0) on
    // UpdateConfiguration when `server` drops.
}

/// The human's bare `done` path (no `--session`, pointer on the target) is
/// unaffected by the fix above: `via` is `GlobalPointer`, `session_tracks_target`
/// is never even evaluated (no session to query), and the flush still runs
/// with no `sessionId` on the wire.
#[tokio::test]
async fn done_with_task_and_no_session_still_flushes_the_humans_own_window() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("CompleteTask"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "completeTask": {
                "id": task_id, "title": "Auth migration",
                "sourceId": "AP-1234", "status": "DONE" } }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "configuration": {
                "aplan.active_task_id": task_id } }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(NoSessionIdOnTheWire)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": null }
        })))
        .expect(1)
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
    aplan_no_session()
        .args(["--api-url", &url, "done", task_id])
        .assert()
        .success();
    // wiremock verifies .expect(1) on FlushWorklogTime when `server` drops.
}

// ---------------------------------------------------------------------------
// Task 3 — `start`, `stop` and `flush` act on the session that is asking.
// Plan 1 deliberately deferred this: none of the three received `--session`
// at all, so `aplan flush --session s1 <task>` parsed but silently dropped
// the session, flushing the human's own window instead. Task 5's hook cannot
// be correct until these three carry the session through.
// ---------------------------------------------------------------------------

/// Case 1: `aplan --session s1 start <task>` binds the session via
/// `BindSession` and must never touch the human's `aplan.active_task_id` —
/// `.expect(0)` on `UpdateConfiguration` is the regression gate for that.
#[tokio::test]
async fn start_with_session_binds_it_without_moving_the_human_pointer() {
    let target = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("BindSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "bindSession": {
                "session": { "id": "s1", "taskId": target, "mode": "TRACKING",
                             "label": null, "endedAt": null },
                "previousTaskId": null } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    // The human's pointer must not move: a session's start writes nothing here.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .expect(0)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "start", target])
        .assert()
        .success();
    // wiremock verifies .expect(1) on BindSession and .expect(0) on
    // UpdateConfiguration when `server` drops.
}

/// Case 2: `aplan start <task>` with no session keeps today's behaviour —
/// `UpdateConfiguration` sets `aplan.active_task_id` — and must never bind a
/// session. No `BindSession` mock is mounted: an errant bind would 404
/// instead of passing quietly.
#[tokio::test]
async fn start_without_session_sets_the_human_pointer_and_does_not_bind_a_session() {
    let target = "00000000-0000-0000-0000-000000000001";
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
        .and(wiremock::matchers::body_string_contains(r#""key":"aplan.active_task_id""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .and(wiremock::matchers::body_string_contains(r#""key":"aplan.active_since""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    // No `BindSession` mock mounted: with no session, binding one would be a bug.
    aplan()
        .args(["--api-url", &url, "start", target])
        .assert()
        .success();
    // wiremock verifies .expect(1) on the aplan.active_task_id write when
    // `server` drops.
}

/// Case 3: `aplan --session s1 stop` flushes the session's own task against
/// its own window (carrying `"sessionId":"s1"`), then closes it via
/// `EndSession` — and must never clear the human's `aplan.active_task_id`.
#[tokio::test]
async fn stop_with_session_flushes_and_ends_it_without_touching_the_human_pointer() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("ClaudeSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body(
            "TRACKING",
            Some(task_id),
        )))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(wiremock::matchers::body_string_contains(r#""sessionId":"s1""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("EndSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "endSession": { "id": "s1", "endedAt": "2026-08-06T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    // The human's pointer must not be cleared by a session's stop.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .expect(0)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "--session", "s1", "stop"])
        .assert()
        .success();
    // wiremock verifies .expect(1) on FlushWorklogTime and EndSession, and
    // .expect(0) on UpdateConfiguration, when `server` drops.
}

/// Case 4: `aplan stop` with no session keeps today's behaviour — flush with
/// no session id, pointer cleared — and must never end a session. No
/// `EndSession` mock is mounted: an errant end would 404 instead of passing
/// quietly.
#[tokio::test]
async fn stop_without_session_flushes_with_no_session_id_and_does_not_end_a_session() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "configuration": { "aplan.active_task_id": task_id } }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(NoSessionIdOnTheWire)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("UpdateConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "updateConfiguration": true }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    // No `EndSession` mock mounted: with no session, ending one would be a bug.
    aplan()
        .args(["--api-url", &url, "stop"])
        .assert()
        .success();
    // wiremock verifies .expect(1) on FlushWorklogTime (no sessionId) and on
    // UpdateConfiguration when `server` drops.
}

/// Case 5: `aplan --session s1 flush <task>` must carry `"sessionId":"s1"` on
/// the wire. Before the fix, `commands.rs` passed `session_id: None` as a
/// literal regardless of `--session`.
#[tokio::test]
async fn flush_with_session_carries_the_session_id_on_the_wire() {
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(wiremock::matchers::body_string_contains(r#""sessionId":"s1""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args([
            "--api-url", &url, "--session", "s1", "flush",
            "00000000-0000-0000-0000-000000000001",
        ])
        .assert()
        .success();
    // wiremock verifies .expect(1) on the sessionId-carrying flush when
    // `server` drops.
}

/// Case 6: `aplan flush <task>` with no session keeps today's behaviour — no
/// string-valued `sessionId` on the wire. `flush` never calls
/// `UpdateConfiguration` at all, so there is nothing to assert `.expect(0)`
/// on there; the server-side watermark choice is covered by plan 2's API
/// tests, not here.
#[tokio::test]
async fn flush_without_session_carries_no_session_id_on_the_wire() {
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(NoSessionIdOnTheWire)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "flush", "00000000-0000-0000-0000-000000000001"])
        .assert()
        .success();
    // wiremock verifies .expect(1) on the sessionId-less flush when `server` drops.
}

// ---------------------------------------------------------------------------
// Task 3, case 7 — `CLAUDE_CODE_SESSION_ID=""` (present but empty, the shape
// a hook running outside any Claude session produces) must behave exactly
// like an absent `--session` for all three commands, mirroring the `log`
// contract pinned above at `an_empty_session_env_var_falls_back_...`.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_with_empty_session_env_behaves_like_no_session() {
    let target = "00000000-0000-0000-0000-000000000001";
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

    // No `BindSession` mock mounted: resolving a session from "" would be a bug.
    aplan()
        .env("CLAUDE_CODE_SESSION_ID", "")
        .args(["--api-url", &url, "start", target])
        .assert()
        .success()
        .stdout(predicate::str::contains("▶ tracking"));
}

#[tokio::test]
async fn stop_with_empty_session_env_behaves_like_no_session() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("GetConfiguration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "configuration": { "aplan.active_task_id": task_id } }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(NoSessionIdOnTheWire)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
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

    // No `ClaudeSession`/`EndSession` mock mounted: resolving a session from
    // "" would be a bug.
    aplan()
        .env("CLAUDE_CODE_SESSION_ID", "")
        .args(["--api-url", &url, "stop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tracking cleared"));
}

#[tokio::test]
async fn flush_with_empty_session_env_behaves_like_no_session() {
    let server = MockServer::start().await;
    mount_get_task(&server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(NoSessionIdOnTheWire)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
        })))
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .env("CLAUDE_CODE_SESSION_ID", "")
        .args(["--api-url", &url, "flush", "00000000-0000-0000-0000-000000000001"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Task 3 review round 1 — `aplan session end` must flush before it closes.
// `EndSession` performs no flush of its own, and once the row is closed no
// future window will ever select this session's entries again: the time
// would be gone for good, not delayed. `stop --session` already got this
// right; `session end` did not, and the two had already diverged.
// ---------------------------------------------------------------------------

/// Two independent `.expect(1)`s cannot tell "flushed, then ended" apart
/// from "ended, then flushed too late to matter" — both would satisfy them
/// identically. This test checks the wire order directly against
/// `MockServer::received_requests()` instead.
#[tokio::test]
async fn session_end_flushes_its_own_task_before_closing_it() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("ClaudeSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body(
            "TRACKING",
            Some(task_id),
        )))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(wiremock::matchers::body_string_contains(r#""sessionId":"s1""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("EndSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "endSession": { "id": "s1", "endedAt": "2026-08-06T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "session", "end", "--session", "s1"])
        .assert()
        .success();

    let received = server
        .received_requests()
        .await
        .expect("request recording is on by default");
    let flush_at = received
        .iter()
        .position(|r| String::from_utf8_lossy(&r.body).contains("FlushWorklogTime"))
        .expect("a FlushWorklogTime request was made");
    let end_at = received
        .iter()
        .position(|r| String::from_utf8_lossy(&r.body).contains("EndSession"))
        .expect("an EndSession request was made");
    assert!(
        flush_at < end_at,
        "FlushWorklogTime (index {flush_at}) must precede EndSession (index {end_at})"
    );
}

// ---------------------------------------------------------------------------
// Task 3 review round 2 — `session off` does not flush, so a session
// switched off before it is ended still owes a flush of whatever it logged
// while `on`. `try_session_task_id` (used by `done`'s attribution gate) is
// deliberately gated on `mode == TRACKING`; closing must not reuse that gate,
// or the exact permanent loss fix round 1 closed reopens through `off`.
// ---------------------------------------------------------------------------

/// Mirrors `session_end_flushes_its_own_task_before_closing_it`, but the
/// session's own row reports `mode: "OFF"` (as it would after `session off`)
/// while still carrying the task it was tracking before the switch. If
/// closing gated the flush on `mode`, this task's time would be silently
/// skipped and then permanently lost the moment `EndSession` succeeds.
#[tokio::test]
async fn session_end_flushes_a_bound_task_even_when_logging_is_off_for_it() {
    let task_id = "00000000-0000-0000-0000-000000000001";
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("ClaudeSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_body("OFF", Some(task_id))))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("FlushWorklogTime"))
        .and(wiremock::matchers::body_string_contains(r#""sessionId":"s1""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "flushWorklogTime": { "slotsWritten": 1, "activeSince": "2026-08-05T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("EndSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "endSession": { "id": "s1", "endedAt": "2026-08-06T09:00:00+00:00" } }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "session", "end", "--session", "s1"])
        .assert()
        .success();

    let received = server
        .received_requests()
        .await
        .expect("request recording is on by default");
    let flush_at = received
        .iter()
        .position(|r| String::from_utf8_lossy(&r.body).contains("FlushWorklogTime"))
        .expect("a FlushWorklogTime request was made");
    let end_at = received
        .iter()
        .position(|r| String::from_utf8_lossy(&r.body).contains("EndSession"))
        .expect("an EndSession request was made");
    assert!(
        flush_at < end_at,
        "FlushWorklogTime (index {flush_at}) must precede EndSession (index {end_at})"
    );
}

/// The refusal `end_session_flushing_first` introduced in round 1 had no
/// test: when the task lookup itself fails (here, `ClaudeSession` returns
/// HTTP 500), closing must refuse rather than treat the failure as "nothing
/// to flush" and end anyway. `.expect(0)` on `EndSession` is the regression
/// gate — this is the exact guard that prevents the permanent loss above,
/// so an untested guard here is a guard that can silently stop working.
#[tokio::test]
async fn session_end_refuses_to_close_when_the_task_lookup_fails() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("ClaudeSession"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("EndSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "endSession": { "id": "s1", "endedAt": "2026-08-06T09:00:00+00:00" } }
        })))
        .expect(0)
        .mount(&server)
        .await;
    let url = format!("{}/graphql", server.uri());

    aplan_no_session()
        .args(["--api-url", &url, "session", "end", "--session", "s1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
    // wiremock verifies .expect(0) on EndSession when `server` drops.
}
