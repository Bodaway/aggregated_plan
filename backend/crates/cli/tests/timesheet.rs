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
                "quarters": [],
                        "lanes": [],
                        "outsideWorkday": []
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
            "aplan timesheet set --quarter <1-4> <task> <hours>",
        ))
        // total line (4.0 + 2.5 = 6.5)
        .stdout(predicate::str::contains("total 6.50h"))
        // `mock_graphql` here answers *every* operation with this same
        // `runTimesheetReconstruction`-shaped body, so the overlap-gap
        // check's own round trip (`ActivityJournal`, not
        // `ActivityOverlaps` — see `integration.rs`'s Task 9 section)
        // gets a response it cannot deserialize and fails — noted on
        // stderr since production commit 6029264 rather than swallowed.
        .stderr(predicate::str::contains("note: overlap check unavailable"));
}

// ---------------------------------------------------------------------------
// `timesheet set` resolves the lane from the day's own reconstruction, then pins
// it inside one quarter. It reconstructs on purpose: pins live on the quarter
// shares and survive a rebuild, so there is nothing left to protect them from.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn timesheet_set_resolves_the_lane_by_title_and_pins_the_quarter() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("ReconstructTimesheet"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "runTimesheetReconstruction": {
                    "date": "2026-07-06",
                    "status": "DRAFT",
                    "targetHours": 8.0,
                    "roundingIncrement": 0.25,
                    "totalHours": 8.0,
                    "dayConfidence": "HIGH",
                    "unattributedHours": 0.0,
                    "lines": [],
                    "unresolved": [],
                    "quarters": [{
                        "index": 3,
                        "startMin": 900,
                        "endMin": 1020,
                        "hours": 2.0,
                        "oooHours": 0.0,
                        "declarableHours": 2.0,
                        "confidence": "HIGH",
                        "shares": [{
                            "laneKey": "task:11111111-1111-1111-1111-111111111111",
                            "taskId": "11111111-1111-1111-1111-111111111111",
                            "label": "Anonymisation eActions",
                            "gryzzlyProjectId": "A",
                            "presenceMinutes": 80,
                            "hours": 2.0,
                            "isPinned": false
                        }]
                    }],
                    "lanes": [],
                    "outsideWorkday": []
                }
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("SetQuarterShare"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "setQuarterShare": {
                    "date": "2026-07-06",
                    "status": "DRAFT",
                    "totalHours": 8.0,
                    "targetHours": 8.0,
                    "quarters": [{
                        "index": 3,
                        "declarableHours": 2.0,
                        "shares": [{
                            "laneKey": "task:11111111-1111-1111-1111-111111111111",
                            "label": "Anonymisation eActions",
                            "hours": 1.5,
                            "isPinned": true
                        }]
                    }]
                }
            }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "timesheet", "set", "--quarter", "4", "anonymisation", "1.5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Q4"))
        .stdout(predicate::str::contains("1.50h"));

    let requests = server.received_requests().await.expect("requests are recorded");
    let pin = requests
        .iter()
        .filter_map(|r| r.body_json::<serde_json::Value>().ok())
        .find(|b| b.get("operationName").and_then(|v| v.as_str()) == Some("SetQuarterShare"))
        .expect("the pin must be sent");
    let vars = &pin["variables"];
    assert_eq!(
        vars["laneKey"], "task:11111111-1111-1111-1111-111111111111",
        "a title must resolve to the lane key, never be sent as-is"
    );
    assert_eq!(vars["quarterIndex"], 3, "`--quarter 4` is the fourth quarter, index 3");
    assert_eq!(vars["hours"], 1.5);
}

/// A title matching two lanes is exit 3 with the candidates listed — these hours reach
/// a client invoice, so a guess is worse than a refusal.
#[tokio::test]
async fn timesheet_set_refuses_an_ambiguous_title() {
    let server = MockServer::start().await;
    let share = |id: &str, label: &str| json!({
        "laneKey": format!("task:{id}"),
        "taskId": id,
        "label": label,
        "gryzzlyProjectId": "A",
        "presenceMinutes": 40,
        "hours": 1.0,
        "isPinned": false
    });
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_string_contains("ReconstructTimesheet"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "runTimesheetReconstruction": {
                    "date": "2026-07-06",
                    "status": "DRAFT",
                    "targetHours": 8.0,
                    "roundingIncrement": 0.25,
                    "totalHours": 8.0,
                    "dayConfidence": "HIGH",
                    "unattributedHours": 0.0,
                    "lines": [],
                    "unresolved": [],
                    "quarters": [{
                        "index": 3,
                        "startMin": 900,
                        "endMin": 1020,
                        "hours": 2.0,
                        "oooHours": 0.0,
                        "declarableHours": 2.0,
                        "confidence": "HIGH",
                        "shares": [
                            share("11111111-1111-1111-1111-111111111111", "SAFT cadrage"),
                            share("22222222-2222-2222-2222-222222222222", "SAFT GitHub Action")
                        ]
                    }],
                    "lanes": [],
                    "outsideWorkday": []
                }
            }
        })))
        .mount(&server)
        .await;

    let url = format!("{}/graphql", server.uri());
    aplan()
        .args(["--api-url", &url, "timesheet", "set", "--quarter", "4", "saft", "1.5"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("ambiguous"))
        .stderr(predicate::str::contains("SAFT cadrage"));
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
