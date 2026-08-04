//! `aplan sessions` / `aplan session <action>` — the per-session link a Claude
//! sees and manages, next to the global pointer the human uses by hand.

use crate::cli::SessionAction;
use crate::client::Client;
use crate::commands::flush_task;
use crate::lookup::{resolve_task, LookupError};
use crate::output::{print_json, ExitCode};
use crate::queries::{
    bind_session, claude_session, end_session, open_claude_sessions, set_session_mode,
    BindSession, ClaudeSession, EndSession, OpenClaudeSessions, SetSessionMode,
};

/// The message every branch that needs a session id and has none exits with.
/// A command that silently did nothing is how the original bug stayed invisible.
const NO_SESSION_ID: &str = "no session id (pass --session or run inside Claude Code)";

/// `HH:MM` out of an ISO-8601 timestamp. Falls back to the raw string if it is
/// shorter than expected rather than panicking on a slice out of range.
fn hhmm(iso: &str) -> &str {
    iso.get(11..16).unwrap_or(iso)
}

/// `aplan sessions` — every open Claude session and what it is working on,
/// plus a display-only line for the global pointer: the human, working by
/// hand, who has no session row and never will.
pub fn sessions(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let result = client.run::<OpenClaudeSessions>(open_claude_sessions::Variables {});
    match result {
        Ok(r) => {
            let manual_task = resolve_task(&client, None).ok();

            if json {
                let manual = match &manual_task {
                    Some(t) => serde_json::json!({ "taskId": t.id, "title": t.title }),
                    None => serde_json::Value::Null,
                };
                let mut payload = r.raw;
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("manual".to_string(), manual);
                }
                if let Err(e) = print_json(&payload) {
                    eprintln!("error writing output: {}", e);
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }

            for s in &r.data.open_claude_sessions {
                let bullet = if matches!(s.mode, open_claude_sessions::SessionModeGql::TRACKING) {
                    "\u{25cf}"
                } else {
                    "\u{25cb}"
                };
                let what = match &s.task {
                    Some(t) => t.title.clone(),
                    None => "not tracking".to_string(),
                };
                let label = s.label.as_deref().unwrap_or("");
                println!(
                    "{bullet} {}  {}  (depuis {}, vu {})  {}",
                    s.id,
                    what,
                    hhmm(&s.started_at),
                    hhmm(&s.last_seen_at),
                    label
                );
            }
            let manual_title = manual_task
                .as_ref()
                .map(|t| t.title.as_str())
                .unwrap_or("(no task)");
            println!("\u{25cb} manuel (toi)  {}", manual_title);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

/// `aplan session show|bind|off|end` — manage `session_id`'s aplan link.
pub fn session(
    api_url: &str,
    json: bool,
    session_id: Option<&str>,
    action: &SessionAction,
) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let sid = session_id.filter(|s| !s.trim().is_empty());

    match action {
        SessionAction::Show => {
            let Some(sid) = sid else {
                eprintln!("error: {}", NO_SESSION_ID);
                return ExitCode::PreconditionFailed;
            };
            match client.run::<ClaudeSession>(claude_session::Variables { id: sid.to_string() }) {
                Ok(r) => {
                    if json {
                        if let Err(e) = print_json(&r.raw) {
                            eprintln!("error writing output: {}", e);
                            return ExitCode::Generic;
                        }
                        return ExitCode::Success;
                    }
                    match r.data.claude_session {
                        None => {
                            eprintln!("error: {}", LookupError::SessionUnknown(sid.to_string()));
                            ExitCode::NotFound
                        }
                        Some(s) => {
                            println!("session {}", s.id);
                            println!("mode: {:?}", s.mode);
                            match s.task_id.filter(|t| !t.is_empty()) {
                                Some(tid) => println!("task: {}", tid),
                                None => println!("task: (none)"),
                            }
                            if let Some(ended) = &s.ended_at {
                                println!("ended: {}", ended);
                            }
                            ExitCode::Success
                        }
                    }
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    ExitCode::Generic
                }
            }
        }
        SessionAction::Bind { task, label } => {
            let Some(sid) = sid else {
                eprintln!("error: {}", NO_SESSION_ID);
                return ExitCode::PreconditionFailed;
            };
            let target = match resolve_task(&client, Some(task)) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return e.exit_code();
                }
            };
            let resolved_label = label.clone().unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            });
            let result = client.run::<BindSession>(bind_session::Variables {
                session_id: sid.to_string(),
                task_id: target.id.clone(),
                label: Some(resolved_label),
            });
            match result {
                Ok(r) => {
                    // Same call `aplan start` makes: time behaviour is unchanged. This
                    // must run before the `--json` branch's early return, not after it
                    // — `--json` is exactly the path the (future) hooks will use, and a
                    // rebind that skips it loses the previous task's time silently.
                    if let Some(prev) = &r.data.bind_session.previous_task_id {
                        flush_task(&client, prev);
                    }
                    if json {
                        if let Err(e) = print_json(&r.raw) {
                            eprintln!("error writing output: {}", e);
                            return ExitCode::Generic;
                        }
                        return ExitCode::Success;
                    }
                    println!("\u{25b6} session {} \u{2192} {}", sid, target.title);
                    ExitCode::Success
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    ExitCode::Generic
                }
            }
        }
        SessionAction::Off => {
            let Some(sid) = sid else {
                eprintln!("error: {}", NO_SESSION_ID);
                return ExitCode::PreconditionFailed;
            };
            let result = client.run::<SetSessionMode>(set_session_mode::Variables {
                session_id: sid.to_string(),
                mode: set_session_mode::SessionModeGql::OFF,
                label: None,
            });
            match result {
                Ok(r) => {
                    if json {
                        if let Err(e) = print_json(&r.raw) {
                            eprintln!("error writing output: {}", e);
                            return ExitCode::Generic;
                        }
                        return ExitCode::Success;
                    }
                    println!(
                        "\u{25cb} session {}: not tracking (aplan logging off for this session)",
                        r.data.set_session_mode.id
                    );
                    ExitCode::Success
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    ExitCode::Generic
                }
            }
        }
        SessionAction::End => {
            let Some(sid) = sid else {
                eprintln!("error: {}", NO_SESSION_ID);
                return ExitCode::PreconditionFailed;
            };
            let result = client.run::<EndSession>(end_session::Variables {
                session_id: sid.to_string(),
            });
            match result {
                Ok(r) => {
                    if json {
                        if let Err(e) = print_json(&r.raw) {
                            eprintln!("error writing output: {}", e);
                            return ExitCode::Generic;
                        }
                        return ExitCode::Success;
                    }
                    match r.data.end_session {
                        Some(s) => println!("\u{25a0} session {} closed", s.id),
                        None => println!("\u{25a0} session {} was already closed", sid),
                    }
                    ExitCode::Success
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    ExitCode::Generic
                }
            }
        }
    }
}
