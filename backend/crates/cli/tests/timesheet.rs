//! Integration tests for `aplan timesheet` (+ `validate` / `set` / `off`).
//!
//! Mirrors the setup in `tests/integration.rs`: a `wiremock` server stubs the
//! GraphQL operations the command issues, and `assert_cmd` drives the real
//! `aplan` binary, asserting on stdout/stderr/exit code.

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::json;
use std::process::Command;
use wiremock::matchers::{body_partial_json, method, path};
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

/// Canned `runTimesheetReconstruction` payload: 2 attributed lines + unattributed hours.
fn reconstruct_day_json() -> serde_json::Value {
    json!({
        "data": {
            "runTimesheetReconstruction": {
                "date": "2026-07-06",
                "status": "DRAFT",
                "targetHours": 8.0,
                "roundingIncrement": 0.25,
                "totalHours": 6.5,
                "dayConfidence": "MEDIUM",
                "unattributedHours": 1.5,
                "lines": [
                    {
                        "gryzzlyProjectId": "PROJ-A",
                        "projectName": "Project Alpha",
                        "hours": 4.0,
                        "isPinned": false,
                        "confidence": "HIGH"
                    },
                    {
                        "gryzzlyProjectId": "PROJ-B",
                        "projectName": "Project Beta",
                        "hours": 2.5,
                        "isPinned": true,
                        "confidence": "MEDIUM"
                    }
                ],
                "unresolved": [],
                "blocks": []
            }
        }
    })
}

#[tokio::test]
async fn timesheet_json_emits_raw_reconstruction_payload() {
    let server = mock_graphql(reconstruct_day_json()).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "--json", "timesheet"])
        .assert()
        .success()
        .stdout(predicate::str::contains("runTimesheetReconstruction"))
        .stdout(predicate::str::contains("PROJ-A"))
        .stdout(predicate::str::contains("PROJ-B"));
}

#[tokio::test]
async fn timesheet_default_render_shows_projects_unattributed_and_total() {
    let server = mock_graphql(reconstruct_day_json()).await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "timesheet"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PROJ-A"))
        .stdout(predicate::str::contains("Project Alpha"))
        .stdout(predicate::str::contains("PROJ-B"))
        .stdout(predicate::str::contains("Project Beta"))
        // unattributed-hours hint line
        .stdout(predicate::str::contains("1.50h unattributed"))
        .stdout(predicate::str::contains(
            "aplan timesheet set <project> <hours>",
        ))
        // total line (4.0 + 2.5 = 6.5)
        .stdout(predicate::str::contains("total 6.50h"));
}

// ---------------------------------------------------------------------------
// Pin-preservation regression: `timesheet set` must load current lines from
// the persisted `timesheetDraft` query (which preserves `isPinned`) and must
// NOT call `runTimesheetReconstruction` when a draft already exists — that
// mutation upserts a fresh draft and would wipe any previously saved pins.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn timesheet_set_preserves_prior_pin_and_does_not_reconstruct() {
    let server = MockServer::start().await;

    // 1) TimesheetDraft query: a draft already exists with project A pinned at 3.0h.
    //    Matched on the exact `operationName` (not a substring) so it can't be
    //    confused with the `SaveTimesheetDraft` mutation below (whose name
    //    contains "TimesheetDraft" as a substring).
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_partial_json(json!({ "operationName": "TimesheetDraft" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "timesheetDraft": {
                    "date": "2026-07-06",
                    "status": "DRAFT",
                    "targetHours": 8.0,
                    "roundingIncrement": 0.25,
                    "totalHours": 3.0,
                    "dayConfidence": "HIGH",
                    "unattributedHours": 5.0,
                    "lines": [
                        {
                            "gryzzlyProjectId": "A",
                            "projectName": "Project A",
                            "hours": 3.0,
                            "isPinned": true,
                            "confidence": "HIGH"
                        }
                    ],
                    "unresolved": [],
                    "blocks": []
                }
            }
        })))
        .mount(&server)
        .await;

    // 2) runTimesheetReconstruction MUST NOT be called while a draft exists.
    //    If `set` regresses to reconstruct-first, this responds with a
    //    GraphQL error so the command fails loudly instead of silently
    //    succeeding with a wiped-pin behavior.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("ReconstructTimesheet"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "errors": [{
                "message": "regression: `timesheet set` must not reconstruct when a draft already exists (would wipe prior pins)"
            }]
        })))
        .mount(&server)
        .await;

    // 3) SaveTimesheetDraft: accept whatever is sent; the pin-preservation
    //    assertion happens below by inspecting the recorded request body.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("SaveTimesheetDraft"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "saveTimesheetDraft": {
                    "date": "2026-07-06",
                    "status": "DRAFT",
                    "totalHours": 8.0,
                    "targetHours": 8.0,
                    "lines": [
                        {
                            "gryzzlyProjectId": "A",
                            "projectName": "Project A",
                            "hours": 3.0,
                            "isPinned": true
                        },
                        {
                            "gryzzlyProjectId": "B",
                            "projectName": null,
                            "hours": 5.0,
                            "isPinned": true
                        }
                    ]
                }
            }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "timesheet", "set", "B", "5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pinned"))
        .stdout(predicate::str::contains("5.00h"));

    // Inspect exactly what was sent to the API.
    let requests = server
        .received_requests()
        .await
        .expect("request recording is enabled by default");

    let op_name = |body: &serde_json::Value| -> Option<String> {
        body.get("operationName")
            .and_then(|v| v.as_str())
            .map(String::from)
    };

    let reconstruct_called = requests.iter().any(|r| {
        r.body_json::<serde_json::Value>()
            .ok()
            .and_then(|b| op_name(&b))
            .as_deref()
            == Some("ReconstructTimesheet")
    });
    assert!(
        !reconstruct_called,
        "`timesheet set` called runTimesheetReconstruction even though a draft already \
         existed — this wipes prior pins and is the exact regression this test guards against"
    );

    let save_request = requests
        .iter()
        .find(|r| {
            r.body_json::<serde_json::Value>()
                .ok()
                .and_then(|b| op_name(&b))
                .as_deref()
                == Some("SaveTimesheetDraft")
        })
        .expect("SaveTimesheetDraft was never called");
    let save_body: serde_json::Value = save_request
        .body_json()
        .expect("SaveTimesheetDraft request body is valid JSON");
    let lines = save_body["variables"]["lines"]
        .as_array()
        .expect("variables.lines is an array");

    assert_eq!(
        lines.len(),
        2,
        "expected the prior pinned line (A) plus the newly-set line (B), got: {lines:?}"
    );
    assert_eq!(lines[0]["gryzzlyProjectId"], json!("A"));
    assert_eq!(lines[0]["hours"], json!(3.0));
    assert_eq!(
        lines[0]["isPinned"],
        json!(true),
        "the prior pin on project A must survive `set` on a different project"
    );
    assert_eq!(lines[1]["gryzzlyProjectId"], json!("B"));
    assert_eq!(lines[1]["hours"], json!(5.0));
    assert_eq!(lines[1]["isPinned"], json!(true));
}

#[tokio::test]
async fn timesheet_off_am_marks_day_off_and_warns_half_day_not_honored() {
    let server = mock_graphql(json!({
        "data": {
            "markDayOff": {
                "date": "2026-07-06",
                "status": "DAY_OFF",
                "totalHours": 0.0
            }
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "timesheet", "off", "--am"])
        .assert()
        .success()
        .stdout(predicate::str::contains("marked off"))
        .stderr(predicate::str::contains(
            "half-day off is not yet honored",
        ));
}

#[tokio::test]
async fn timesheet_validate_confirms_validation() {
    let server = mock_graphql(json!({
        "data": {
            "validateTimesheet": {
                "date": "2026-07-06",
                "status": "VALIDATED",
                "totalHours": 8.0
            }
        }
    }))
    .await;
    let url = format!("{}/graphql", server.uri());

    aplan()
        .args(["--api-url", &url, "timesheet", "validate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("validated"));
}
