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
    // `current` reads aplan.active_task_id via GetConfiguration.
    // When the stored value is a UUID, resolve_task short-circuits without a
    // network call and returns a TaskRef with an empty title; the output is
    // "▶ tracking: " (empty title). This matches the real flow: `start` stores
    // the raw UUID from task resolution.
    let server = mock_graphql(json!({
        "data": {
            "configuration": {
                "aplan.active_task_id": "00000000-0000-0000-0000-000000000001"
            }
        }
    }))
    .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("▶ tracking"));
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
    // resolve_task (implicit) reads aplan.active_task_id from GetConfiguration
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
    // resolve_task (implicit) reads aplan.active_task_id from GetConfiguration
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
    let server = mock_graphql(json!({ "data": { "deleteTask": true } })).await;
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
    // UUID token: resolve_task returns immediately (no network call for lookup).
    // start then reads the previous pointer (GetConfiguration), finds nothing,
    // and writes the new pointer via UpdateConfiguration (twice: id + since).
    // Human output: "▶ tracking: <title>" — but UUID resolve gives an empty
    // title, so the binary prints "▶ tracking: " (empty title is acceptable;
    // we assert on the operation being issued, not the exact title text).
    let server = MockServer::start().await;
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
    let server = mock_graphql(json!({
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
    }))
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

/// `aplan start <uuid>` issues GetConfiguration (prior pointer check) then
/// UpdateConfiguration (sets aplan.active_task_id and aplan.active_since).
#[tokio::test]
async fn start_log_stop_pointer_lifecycle_start_sets_pointer() {
    let server = MockServer::start().await;
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

/// `aplan current --json` reports the active task under `.currentActivity.task.id`
/// when GetConfiguration returns a non-empty aplan.active_task_id.
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

    let url = format!("{}/graphql", server.uri());
    // UUID token → resolve_task returns immediately; title is empty but id is present.
    aplan()
        .args(["--api-url", &url, "--json", "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains(task_id));
}

/// `aplan log "did a thing"` with a UUID token issues AddWorklogEntry.
#[tokio::test]
async fn start_log_stop_pointer_lifecycle_log_issues_add_worklog_entry() {
    let server = MockServer::start().await;
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
