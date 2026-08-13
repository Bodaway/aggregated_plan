//! Subcommand implementations. Each function takes the parsed `Cli` for global
//! flags (api_url, json) and returns an exit code.

use crate::cli::{
    ConfigCmd, ImpactArg, SourceArg, StatusArg, TriageArg, UrgencyArg, WorklogAmount,
};
use crate::client::{Client, ClientError, RunResult};
use crate::lookup::{
    resolve_target, resolve_task, session_task_id, task_id_to_flush_before_closing, LookupError,
    ResolvedVia,
};
use crate::output::{print_json, ExitCode};
use crate::queries::{
    activity_journal, activity_overlaps, add_worklog_entry, append_task_notes, bind_session,
    complete_task, create_task, daily_dashboard, delete_task, end_session, flush_worklog_time,
    force_sync, get_configuration, get_task, list_alerts, list_tasks, priority_matrix,
    rebuild_worklog_projection, reset_urgency, resolve_alert, set_tracking_state, task_worklog,
    update_configuration, update_priority, update_task_status, ActivityJournal, ActivityOverlaps,
    AddWorklogEntry, AppendTaskNotes, BindSession, CompleteTask, CreateTask, DailyDashboard,
    DeleteTask, EndSession, FlushWorklogTime, ForceSync, GetConfiguration, GetTask, ListAlerts,
    ListTasks, PriorityMatrix, RebuildWorklogProjection, ResetUrgency, ResolveAlert,
    SetTrackingState, TaskWorklog, UpdateConfiguration, UpdatePriority, UpdateTaskStatus,
};

/// Who authored a row that carries a `session_id`, as the pinned overlap-line
/// spec prints it: `manuel` for the human (no session id), else the first 4
/// characters of the session id.
///
/// Contrary to the brief's claim, `aplan sessions` does *not* abbreviate —
/// it prints the full id (`session_cmd.rs:63`). This 4-character form was
/// introduced here to match the pinned overlap-line spec exactly, and
/// `show`'s worklog reuses it so the same session reads the same in both
/// places.
fn overlap_actor_label(session_id: &Option<String>) -> String {
    match session_id {
        Some(id) => id.chars().take(4).collect(),
        None => "manuel".to_string(),
    }
}

/// One flagged overlap's display line, e.g.
/// `⚠ recouvrement 47 min — Saft cadrage ↔ Cartier (session a1b2 ↔ manuel)`.
///
/// French, deliberately (see the task-9 brief): the spec's own wording, with
/// precedent (`○ manuel (toi)` in `aplan sessions`), even though the
/// surrounding `journal` labels are English.
fn format_overlap_line(
    minutes: i64,
    title_a: &str,
    title_b: &str,
    session_a: &Option<String>,
    session_b: &Option<String>,
) -> String {
    format!(
        "\u{26a0} recouvrement {minutes} min \u{2014} {title_a} \u{2194} {title_b} (session {} \u{2194} {})",
        overlap_actor_label(session_a),
        overlap_actor_label(session_b),
    )
}

/// `dash`'s one-line summary when the day carries any overlap, e.g.
/// `⚠ 2 recouvrements aujourd'hui (50 min au total) — détail : aplan journal`.
///
/// `total_minutes` is the **sum of the pairs' minutes**, which double-counts
/// a slot involved in two pairs — deliberately: this line reports a
/// magnitude of the problem, not a quantity of time to reconcile (that is
/// `timesheet`'s job, using the union of the slots' intervals instead). Do
/// not "fix" this into the union measure; the two commands answer different
/// questions on purpose.
fn format_dash_overlap_summary(count: usize, total_minutes: i64) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!(
        "\u{26a0} {count} recouvrement{plural} aujourd'hui ({total_minutes} min au total) \u{2014} d\u{e9}tail : aplan journal"
    )
}

/// Read `aplan.active_task_id` from configuration, if set and non-empty.
fn active_task_id(client: &Client) -> Option<String> {
    let r = client
        .run::<GetConfiguration>(get_configuration::Variables {})
        .ok()?;
    r.data
        .configuration
        .get("aplan.active_task_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Set a single config key (best-effort; warns on failure).
fn set_config_key(client: &Client, key: &str, value: &str) {
    if let Err(e) = client.run::<UpdateConfiguration>(update_configuration::Variables {
        key: key.to_string(),
        value: value.to_string(),
    }) {
        eprintln!("warning: failed to set {}: {}", key, e);
    }
}

/// Flush the worklog window of `task_id` into closed activity slots.
///
/// `session`: `None` flushes the human's global pointer — `start`, `done`,
/// and `stop` all read `aplan.active_task_id`, so none of them owns a
/// session's window. `Some(id)` flushes that session's own window instead;
/// `session_cmd::bind` passes its own id so a rebind never consumes the
/// global window it does not own.
pub(crate) fn flush_task(client: &Client, task_id: &str, session: Option<&str>) {
    if let Err(e) = client.run::<FlushWorklogTime>(flush_worklog_time::Variables {
        task_id: task_id.to_string(),
        session_id: session.map(|s| s.to_string()),
    }) {
        eprintln!("warning: failed to flush worklog time: {}", e);
    }
}

/// An empty string is how a hook running outside any Claude session sets
/// `CLAUDE_CODE_SESSION_ID` — treat it exactly like an absent `--session`,
/// never as a session id of `""`. Pinned for `log` at
/// `integration.rs:1379-1414`; `start`, `stop` and `flush` hold the same
/// contract.
pub(crate) fn present_session(session: Option<&str>) -> Option<&str> {
    session.filter(|s| !s.trim().is_empty())
}

/// Bind `session_id` to `task_id`, then flush whatever task the session was
/// previously tracking against *that session's own* window — never the
/// human's `aplan.active_task_id` / `aplan.active_since`. `session bind` and
/// a session's own `start` are the same operation under two names; they
/// share this exactly and differ only in their label default and their
/// success message.
pub(crate) fn bind_session_flushing_previous(
    client: &Client,
    session_id: &str,
    task_id: &str,
    label: Option<String>,
) -> Result<RunResult<bind_session::ResponseData>, ClientError> {
    let r = client.run::<BindSession>(bind_session::Variables {
        session_id: session_id.to_string(),
        task_id: task_id.to_string(),
        label,
    })?;
    if let Some(prev) = &r.data.bind_session.previous_task_id {
        flush_task(client, prev, Some(session_id));
    }
    Ok(r)
}

/// Flush `session_id`'s own bound task (if it has one — regardless of
/// whether logging is currently `off` for it) then close its row via
/// `EndSession`, and report the outcome the same way for every caller.
/// Flushing first is load-bearing: `endSession` performs no flush of its
/// own, and once the row is closed no future window will ever select this
/// session's worklog entries again — that time would be gone for good, not
/// delayed. The lookup is deliberately mode-independent
/// (`task_id_to_flush_before_closing`, not `try_session_task_id`): `mode`
/// says whether the session is currently tracking, not whether the row
/// still holds a `task_id` this close would otherwise lose — the two agree
/// for an `off` session anyway, since `session off` (`setSessionMode`)
/// already flushed and cleared `task_id` up front, so this simply finds
/// nothing left to do for it. Refuses to end (surfaces the lookup's error
/// instead of closing blind) when the task lookup itself failed — ending is
/// irreversible, unlike `done`'s flush gate, which leaves the session open
/// and its time recoverable by a later `stop` or `flush`. And refuses the
/// same way if the flush itself fails, for the same reason (see below).
pub(crate) fn end_session_flushing_first(
    client: &Client,
    session_id: &str,
    json: bool,
) -> ExitCode {
    match task_id_to_flush_before_closing(client, session_id) {
        Ok(Some(tid)) => {
            // Not `flush_task`: that swallows a failed flush into a warning,
            // which is the right call for its other (best-effort) callers
            // but wrong here. Ending is irreversible — once the row closes,
            // no later window ever selects this session's entries again —
            // so a flush that failed must refuse the close, the same way
            // the lookup failure above already does, rather than let it
            // proceed and lose that time for good.
            if let Err(e) = client.run::<FlushWorklogTime>(flush_worklog_time::Variables {
                task_id: tid,
                session_id: Some(session_id.to_string()),
            }) {
                eprintln!("error: {}", e);
                return ExitCode::Generic;
            }
        }
        Ok(None) => {}
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::Generic;
        }
    }
    match client.run::<EndSession>(end_session::Variables {
        session_id: session_id.to_string(),
    }) {
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
                None => println!("\u{25a0} session {} was already closed", session_id),
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn start(api_url: &str, json: bool, task: &str, session: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, Some(task)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };

    let sid = present_session(session);

    if let Some(sid) = sid {
        // A session asked: this is `session bind` under another name — same
        // helper, same message, so the two can never drift apart again. The
        // human's `aplan.active_task_id` / `aplan.active_since` are a
        // different pointer entirely and must not move here. Crossing that
        // boundary is the mirror image of the shared-watermark defect this
        // whole feature exists to fix, so this branch must never call
        // `set_config_key`.
        return match bind_session_flushing_previous(&client, sid, &target.id, None) {
            Ok(r) => {
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
        };
    }

    // No session: today's behaviour exactly — flush and re-arm the human's
    // own pointer, untouched by any session.
    if let Some(prev) = active_task_id(&client) {
        flush_task(&client, &prev, None);
    }
    let now = chrono::Utc::now().to_rfc3339();
    set_config_key(&client, "aplan.active_task_id", &target.id);
    set_config_key(&client, "aplan.active_since", &now);
    if json {
        let payload = serde_json::json!({ "activeTaskId": target.id, "activeSince": now });
        if let Err(e) = print_json(&payload) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }
    println!("▶ tracking: {}", target.title);
    ExitCode::Success
}

pub fn done(
    api_url: &str,
    json: bool,
    task: Option<&str>,
    session: Option<&str>,
    keep_running: bool,
) -> ExitCode {
    let client = Client::new(api_url.to_string());

    // `--task` or `--session` present: go through the three-level resolution order,
    // keeping track of *which* level answered (`via`) — the flush gate below needs
    // it to ask "was this task being tracked by whoever is asking" rather than
    // always consulting the human's global pointer, the same distinction `log`
    // already makes for `sessionId` attribution (see below).
    // Neither present: keep the original lightweight lookup, which reads only the
    // id (the title comes later from `CompleteTask`'s own response) — resolving
    // through `resolve_target` here would additionally hydrate via `GetTask` for
    // no reason. It answers on behalf of the global pointer, same as `resolve_target`
    // would with both arguments absent.
    let has_explicit_target = task.is_some() || present_session(session).is_some();
    let (target_id, via) = if has_explicit_target {
        match resolve_target(&client, session, task) {
            Ok((t, via)) => (t.id, via),
            Err(e) => {
                eprintln!("error: {}", e);
                return e.exit_code();
            }
        }
    } else {
        match active_task_id(&client) {
            Some(id) => (id, ResolvedVia::GlobalPointer),
            None => {
                eprintln!("error: {}", LookupError::NoCurrentActivity);
                return ExitCode::PreconditionFailed;
            }
        }
    };

    // Complete the task
    let (completed, completed_raw) = match client.run::<CompleteTask>(complete_task::Variables {
        id: target_id.clone(),
    }) {
        Ok(r) => (r.data.complete_task, r.raw),
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::Generic;
        }
    };

    // Flush against whoever was tracking this task, then clear the human's
    // pointer iff it was the one pointing at it (unless --keep-running). These
    // are two separate questions: a session-resolved target came *from* that
    // session's own `task_id` (`resolve_from_session` hydrates exactly that id),
    // so the session was tracking it by construction — the flush must carry
    // that session so the server advances its own window, not the human's, and
    // it must do so whether or not the global pointer happens to agree. A task-
    // or pointer-resolved target has no session behind it, so whether it was
    // tracked is still the human pointer question it always was.
    //
    // Clearing `aplan.active_task_id` stays keyed on the pointer alone, on
    // purpose: a session's `done` must never blank the human's unrelated
    // tracking. The two watermarks (`sessions.last_flush_at` and
    // `aplan.active_since`) must never cross.
    // `--task` wins the resolution (`via == Task`) even when it names exactly
    // the task a bound session is tracking — that is what "always wins" means.
    // But it must not cost that session its time: ask once whether `session`
    // (if any) is itself bound to `target_id`, and if so treat this `done`
    // as session-tracked for both the gate and the flush below, same as an
    // implicit `ResolvedVia::Session` resolution would. `--task` still
    // decided *which* task; this only decides *whose* window flushes.
    let session_tracks_target = via == ResolvedVia::Task
        && present_session(session)
            .is_some_and(|id| session_task_id(&client, id).as_deref() == Some(target_id.as_str()));

    let active = active_task_id(&client);
    let pointer_on_target = active.as_deref() == Some(target_id.as_str());
    let was_tracking_target = match via {
        ResolvedVia::Session => true,
        ResolvedVia::Task => pointer_on_target || session_tracks_target,
        ResolvedVia::GlobalPointer => pointer_on_target,
    };
    if !keep_running {
        if was_tracking_target {
            let flush_session = match via {
                ResolvedVia::Session => session.map(|s| s.to_string()),
                ResolvedVia::Task if session_tracks_target => session.map(|s| s.to_string()),
                ResolvedVia::Task | ResolvedVia::GlobalPointer => None,
            };
            flush_task(&client, &target_id, flush_session.as_deref());
        }
        if pointer_on_target {
            set_config_key(&client, "aplan.active_task_id", "");
        }
    }

    if json {
        let completed_json = completed_raw
            .get("completeTask")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let payload = serde_json::json!({
            "completed": completed_json,
            "stoppedMinutes": serde_json::Value::Null,
        });
        if let Err(e) = print_json(&payload) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }

    let label = completed.source_id.as_deref().unwrap_or(&completed.title);
    println!("✓ {} done", label);
    ExitCode::Success
}

pub fn triage(api_url: &str, json: bool, state: &TriageArg, task: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, Some(task)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };
    let result = client.run::<SetTrackingState>(set_tracking_state::Variables {
        task_id: target.id.clone(),
        state: state.as_graphql(),
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
            let label = r
                .data
                .set_tracking_state
                .source_id
                .as_deref()
                .unwrap_or(&r.data.set_tracking_state.title);
            println!(
                "⇄ {}: tracking → {:?}",
                label, r.data.set_tracking_state.tracking_state
            );
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn status(
    api_url: &str,
    json: bool,
    state: &StatusArg,
    task: Option<&str>,
    session: Option<&str>,
) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_target(&client, session, task) {
        Ok((t, _)) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };
    let result = client.run::<UpdateTaskStatus>(update_task_status::Variables {
        id: target.id.clone(),
        status: state.as_graphql(),
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
            let label = r
                .data
                .update_task
                .source_id
                .as_deref()
                .unwrap_or(&r.data.update_task.title);
            println!("↻ {}: status → {:?}", label, r.data.update_task.status);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn note(
    api_url: &str,
    json: bool,
    text: &[String],
    task: Option<&str>,
    session: Option<&str>,
) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_target(&client, session, task) {
        Ok((t, _)) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };
    let joined = text.join(" ");
    let result = client.run::<AppendTaskNotes>(append_task_notes::Variables {
        task_id: target.id.clone(),
        text: joined,
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
            let label = r
                .data
                .append_task_notes
                .source_id
                .as_deref()
                .unwrap_or(&r.data.append_task_notes.title);
            println!("✎ {}: note appended", label);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

/// Where a bare `--at 2026-08-06` lands: local noon.
///
/// Not an arbitrary midpoint. A worklog entry is evidence for the 45 minutes BEFORE
/// it (`domain::rules::presence::build_lanes`), clipped to the configured working
/// windows, so what a bare date bills depends entirely on the hour chosen for it.
/// Noon carries `11:15–12:00` — a full 45 minutes inside the morning window. Both
/// obvious alternatives bill nothing at all: midnight falls in no window, and the
/// workday's own 08:00 start casts its shadow over `07:15–08:00`, entirely before
/// the window opens.
const BARE_DATE_HOUR: u32 = 12;

/// The four forms `--at` accepts, in the order they are tried. Date-and-time first:
/// `%H:%M` would otherwise never be reached, since a `%Y-%m-%d` parse of `14:30`
/// fails but a bare-date branch checked earlier could not tell the two apart.
const AT_DATETIME_FORMATS: [&str; 4] =
    ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"];

/// The local wall-clock reading `--at` names, or a refusal naming the accepted forms.
///
/// Deliberately *local*, and sent to the server as such (`addWorklogEntry`'s
/// `loggedAtLocal`). Converting to UTC here would make this the second
/// implementation of local-to-UTC in the codebase, and the day a disagreement
/// between the two lands on is the day the hours get billed to —
/// `application::time` exists to keep that conversion single.
///
/// `today` is passed in rather than read, so the time-only form is testable.
fn parse_at(value: &str, today: chrono::NaiveDate) -> Result<chrono::NaiveDateTime, String> {
    for format in AT_DATETIME_FORMATS {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, format) {
            return Ok(dt);
        }
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date
            .and_hms_opt(BARE_DATE_HOUR, 0, 0)
            .ok_or_else(|| format!("--at {value} has no noon"));
    }
    for format in ["%H:%M:%S", "%H:%M"] {
        if let Ok(time) = chrono::NaiveTime::parse_from_str(value, format) {
            return Ok(today.and_time(time));
        }
    }
    Err(format!(
        "--at {value} is not a moment (expected YYYY-MM-DDTHH:MM, \"YYYY-MM-DD HH:MM\", \
         HH:MM for today, or YYYY-MM-DD for that day at noon)"
    ))
}

/// The half-days a rebuild touched, as one line: `morning`, `afternoon`,
/// `morning+afternoon`. Shared with `slots_cmd::rebuild`, which reports the same
/// operation run by hand — two spellings of one outcome would read as two outcomes.
pub(crate) fn half_day_labels(half_days: &[rebuild_worklog_projection::HalfDayGql]) -> String {
    use rebuild_worklog_projection::HalfDayGql;
    half_days
        .iter()
        .map(|h| match h {
            HalfDayGql::MORNING => "morning",
            HalfDayGql::AFTERNOON => "afternoon",
            HalfDayGql::Other(_) => "half-day",
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// The `↻ …` line a rebuild reports.
///
/// An empty `halfDays` is unreachable straight after a backdated write — the entry
/// that triggered the rebuild is on that day — but a silent nothing would be
/// indistinguishable from a rebuild that wrote real slots, so it says so.
fn rebuild_line(rebuilt: &rebuild_worklog_projection::RebuildWorklogProjectionRebuildWorklogProjection) -> String {
    if rebuilt.half_days.is_empty() {
        format!("↻ {}: nothing to rebuild", rebuilt.date)
    } else {
        format!(
            "↻ {} {}: {} slot(s) rebuilt",
            rebuilt.date,
            half_day_labels(&rebuilt.half_days),
            rebuilt.slots_written
        )
    }
}

/// Rebuild `task_id`'s activity slots for one local day.
///
/// Split out from [`log`] because the backdated write is already done by the time this
/// runs: a failure here must be reported without claiming the entry failed.
fn rebuild_day(
    client: &Client,
    task_id: &str,
    date: chrono::NaiveDate,
) -> Result<RunResult<rebuild_worklog_projection::ResponseData>, ClientError> {
    client.run::<RebuildWorklogProjection>(rebuild_worklog_projection::Variables {
        task_id: task_id.to_string(),
        date: date.to_string(),
    })
}

pub fn log(
    api_url: &str,
    json: bool,
    text: &[String],
    task: Option<&str>,
    at: Option<&str>,
    session: Option<&str>,
) -> ExitCode {
    // Before the lookup on purpose: a mistyped `--at` is a precondition failure, and
    // finding that out costs no round-trip and writes nothing.
    let at = match at.map(|v| parse_at(v, chrono::Local::now().date_naive())) {
        Some(Ok(parsed)) => Some(parsed),
        Some(Err(message)) => {
            eprintln!("error: {message}");
            return ExitCode::PreconditionFailed;
        }
        None => None,
    };

    let client = Client::new(api_url.to_string());
    let (target, via) = match resolve_target(&client, session, task) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };
    let joined = text.join(" ");
    // `sessionId` is only sent when the session itself named `target` — not for
    // `--task` (which never touches the session) and not for a session id that
    // turned out to be unknown and fell through to the global pointer (see
    // `ResolvedVia`'s doc). `worklog_entries.session_id` is a real foreign key;
    // sending an id with no row would fail the whole write, having logged nothing.
    let session_id = match via {
        ResolvedVia::Session => session.map(|s| s.to_string()),
        ResolvedVia::Task | ResolvedVia::GlobalPointer => None,
    };
    let result = client.run::<AddWorklogEntry>(add_worklog_entry::Variables {
        task_id: target.id.clone(),
        body: joined,
        logged_at_local: at.map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
        session_id,
    });
    let mut raw = match result {
        Ok(r) => r.raw,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::Generic;
        }
    };

    // A backdated entry sits before the flush window, so nothing automatic would ever
    // turn it into slots: `aplan journal` and the workload view would keep showing that
    // day as empty. Rebuilding it here is what makes `--at` place the *hours*, not just
    // the note. Ahead of the `--json` early return on purpose: a machine caller that
    // got the entry written and the day left unrebuilt would be back at the very defect
    // `--at` exists to fix, and silently.
    let mut rebuilt_line = None;
    if let Some(dt) = at {
        match rebuild_day(&client, &target.id, dt.date()) {
            Ok(r) => {
                rebuilt_line = Some(rebuild_line(&r.data.rebuild_worklog_projection));
                // Merged into the same `data` object the entry came back in, so
                // `--json` reports both halves of what `--at` did. Only ever present
                // alongside `--at`, so no existing caller's payload changes shape.
                if let (Some(target), Some(source)) = (raw.as_object_mut(), r.raw.as_object()) {
                    for (key, value) in source {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
            Err(e) => {
                // Success despite the failure, and deliberately: the entry is written
                // and a re-run of this command would duplicate it, which is worse than
                // an unrebuilt day. The rebuild itself is idempotent, so the named
                // repair is safe to run as often as needed.
                eprintln!(
                    "warning: the entry was written, but rebuilding {date} failed: {e}\n\
                     \x20        do NOT re-run this log — it would duplicate the entry.\n\
                     \x20        rebuild alone: aplan slots rebuild --task {id} --date {date}",
                    date = dt.date(),
                    id = target.id,
                );
            }
        }
    }

    if json {
        if let Err(e) = print_json(&raw) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }

    match at {
        Some(dt) => println!(
            "✎ {}: worklog entry added ({})",
            target.title,
            dt.format("%Y-%m-%d %H:%M")
        ),
        None => println!("✎ {}: worklog entry added", target.title),
    }
    if let Some(line) = rebuilt_line {
        println!("{line}");
    }

    ExitCode::Success
}

pub fn stop(api_url: &str, json: bool, session: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let sid = present_session(session);

    if let Some(sid) = sid {
        // A session asked: this is `session end` under another name — same
        // helper, so flush-before-close cannot drift between the two call
        // sites again. The human's `aplan.active_task_id` is a different
        // pointer entirely and must not be cleared by someone else's stop —
        // the mirror image of the shared-watermark defect this whole
        // feature exists to fix.
        return end_session_flushing_first(&client, sid, json);
    }

    // No session: today's behaviour exactly — flush and clear the human's
    // own pointer, untouched by any session.
    let active = active_task_id(&client);
    if let Some(ref tid) = active {
        flush_task(&client, tid, None);
    }
    set_config_key(&client, "aplan.active_task_id", "");
    if json {
        let payload = serde_json::json!({ "stopped": active });
        if let Err(e) = print_json(&payload) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }
    match active {
        Some(_) => println!("⏹ stopped — worklog time flushed, tracking cleared"),
        None => println!("(no task was being tracked)"),
    }
    ExitCode::Success
}

pub fn alerts(api_url: &str, json: bool, all: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let resolved_filter: Option<bool> = if all { None } else { Some(false) };
    let result = client.run::<ListAlerts>(list_alerts::Variables {
        resolved: resolved_filter,
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
            for edge in &r.data.alerts.edges {
                let a = &edge.node;
                println!(
                    "[{:?}] {:?}: {}  ({})",
                    a.severity, a.alert_type, a.message, a.id
                );
            }
            println!("({} alerts)", r.data.alerts.total_count);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn journal(api_url: &str, json: bool, date: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date_str = match date {
        Some(s) => s.to_string(),
        None => chrono::Utc::now().date_naive().to_string(),
    };
    let result = client.run::<ActivityJournal>(activity_journal::Variables {
        date: date_str.clone(),
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
            let total: i64 = r
                .data
                .activity_journal
                .iter()
                .filter_map(|s| s.duration_minutes)
                .sum();
            for slot in &r.data.activity_journal {
                let title = slot
                    .task
                    .as_ref()
                    .map(|t| t.title.as_str())
                    .unwrap_or("(no task)");
                let mins = slot.duration_minutes.unwrap_or(0);
                let h = mins / 60;
                let m = mins % 60;
                println!(
                    "{}  {}  {}h {}m  {}",
                    slot.start_time,
                    slot.end_time.as_deref().unwrap_or("running"),
                    h,
                    m,
                    title
                );
            }
            println!("\ntotal: {}h {}m", total / 60, total % 60);

            // Overlap warnings, one line per pair. A separate round trip on
            // the additive sibling query (task 8) rather than merged into
            // ActivityJournal's own selection: dash/timesheet need the same
            // operation, and merging would have required their responses to
            // always carry a field they don't otherwise use.
            //
            // Best-effort: this is a supplementary check layered on top of
            // the journal that already printed successfully above. A
            // failure here (network hiccup, an older server without the
            // operation) must not turn a working `journal` into a hard
            // error — it only means the warning is silently unavailable
            // this time, not that nothing else printed matters. But the
            // silence must not be *total*: a note on stderr (never stdout,
            // which stays machine-clean) so "no overlaps today" is never
            // indistinguishable from "the check failed" — the one case
            // where a user would misread it is exactly an older server.
            match client.run::<ActivityOverlaps>(activity_overlaps::Variables { date: date_str }) {
                Ok(or) => {
                    // A sub-minute intersection truncates to 0 (task 7) and would
                    // be pure noise here — never printed.
                    let pairs: Vec<_> = or
                        .data
                        .activity_overlaps
                        .into_iter()
                        .filter(|o| o.minutes > 0)
                        .collect();
                    if !pairs.is_empty() {
                        println!();
                        for o in &pairs {
                            let title_a = o
                                .a
                                .task
                                .as_ref()
                                .map(|t| t.title.as_str())
                                .unwrap_or("(no task)");
                            let title_b = o
                                .b
                                .task
                                .as_ref()
                                .map(|t| t.title.as_str())
                                .unwrap_or("(no task)");
                            println!(
                                "{}",
                                format_overlap_line(
                                    o.minutes,
                                    title_a,
                                    title_b,
                                    &o.a.session_id,
                                    &o.b.session_id,
                                )
                            );
                        }
                    }
                }
                Err(e) => eprintln!("note: overlap check unavailable: {}", e),
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn matrix(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let result = client.run::<PriorityMatrix>(priority_matrix::Variables {});
    match result {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {}", e);
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            let m = r.data.priority_matrix;
            println!("\n[URGENT + IMPORTANT] ({})", m.urgent_important.len());
            for t in &m.urgent_important {
                let key = t.source_id.as_deref().unwrap_or("—");
                println!("  {:10} {}", key, t.title);
            }
            println!("\n[IMPORTANT] ({})", m.important.len());
            for t in &m.important {
                let key = t.source_id.as_deref().unwrap_or("—");
                println!("  {:10} {}", key, t.title);
            }
            println!("\n[URGENT] ({})", m.urgent.len());
            for t in &m.urgent {
                let key = t.source_id.as_deref().unwrap_or("—");
                println!("  {:10} {}", key, t.title);
            }
            println!("\n[NEITHER] ({})", m.neither.len());
            for t in &m.neither {
                let key = t.source_id.as_deref().unwrap_or("—");
                println!("  {:10} {}", key, t.title);
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn dash(api_url: &str, json: bool, date: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let date_str = match date {
        Some(s) => s.to_string(),
        None => chrono::Utc::now().date_naive().to_string(),
    };
    let result = client.run::<DailyDashboard>(daily_dashboard::Variables {
        date: date_str.clone(),
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
            let d = r.data.daily_dashboard;
            println!("== {} ==", d.date);
            println!("\ntasks ({}):", d.tasks.len());
            for t in &d.tasks {
                let key = t.source_id.as_deref().unwrap_or("—");
                println!("  {:10} {:?}  {}", key, t.status, t.title);
            }
            println!("\nmeetings ({}):", d.meetings.len());
            for m in &d.meetings {
                println!("  {} → {}  {}", m.start_time, m.end_time, m.title);
            }
            println!("\nalerts ({}):", d.alerts.len());
            for a in &d.alerts {
                println!("  [{:?}] {:?}: {}", a.severity, a.alert_type, a.message);
            }

            // One summary line if the day carries any overlap — never a
            // per-pair breakdown here, that is `journal`'s job. Best-effort,
            // same reasoning as `journal`: a failure here must not turn a
            // working `dash` into a hard error, but must not be silent
            // either — see the note there on why stdout stays clean and
            // stderr gets the note instead.
            match client.run::<ActivityOverlaps>(activity_overlaps::Variables { date: date_str }) {
                Ok(or) => {
                    let pairs: Vec<_> = or
                        .data
                        .activity_overlaps
                        .into_iter()
                        .filter(|o| o.minutes > 0)
                        .collect();
                    if !pairs.is_empty() {
                        let total: i64 = pairs.iter().map(|o| o.minutes).sum();
                        println!();
                        println!("{}", format_dash_overlap_summary(pairs.len(), total));
                    }
                }
                Err(e) => eprintln!("note: overlap check unavailable: {}", e),
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

/// A worklog entry's instant in the reader's local time — `2026-08-12 09:12`.
///
/// `aplan log --at` reads local wall-clock time, so echoing the entry back in
/// the UTC the server stores would make the same instant look like two
/// different ones depending on which verb printed it. A timestamp the server
/// sends in some other shape degrades to its first 16 characters rather than
/// being dropped.
fn worklog_stamp(logged_at: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(logged_at) {
        Ok(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        Err(_) => logged_at
            .chars()
            .take(16)
            .map(|c| if c == 'T' { ' ' } else { c })
            .collect(),
    }
}

/// One worklog line under `aplan show`:
/// `  2026-08-12 09:12  a1b2    Root cause identified: …`
///
/// The actor column is [`overlap_actor_label`], so the session that wrote an
/// entry reads the same here as in a `journal` overlap warning. A body that
/// spans several lines keeps its shape: continuation lines are indented under
/// the first, never re-wrapped and never flattened into one line, because an
/// entry is a quotation of what was logged.
fn format_worklog_entry(logged_at: &str, body: &str, session_id: &Option<String>) -> String {
    let stamp = worklog_stamp(logged_at);
    let actor = overlap_actor_label(session_id);
    let actor_width = actor.chars().count().max(6);
    let indent = 2 + stamp.chars().count() + 2 + actor_width + 2;

    let mut lines = body.lines();
    let head = lines.next().unwrap_or("");
    let mut out = format!("  {stamp}  {actor:<actor_width$}  {head}");
    for line in lines {
        out.push('\n');
        out.push_str(&" ".repeat(indent));
        out.push_str(line);
    }
    out
}

/// A slice of a task's worklog, **oldest first**, so the section reads forwards
/// like the journal it is.
struct WorklogTail {
    entries: Vec<task_worklog::TaskWorklogWorklogEntries>,
    /// Whether entries older than the ones kept exist. Known without a count
    /// query because [`fetch_worklog`] asks for one more row than it shows;
    /// always false for [`WorklogAmount::All`], which left nothing behind.
    older_exist: bool,
    /// The same entries, same order, as raw JSON — what `--json` embeds so the
    /// two modes never disagree about which entries `show` reported.
    raw: serde_json::Value,
}

/// The server's own per-request ceiling (`WORKLOG_FILTER_MAX_LIMIT`). Asking
/// for more than this silently gets this, which is exactly why `--worklog all`
/// pages instead of asking for one enormous page.
const WORKLOG_MAX_PAGE: i64 = 1000;

/// Stops `--worklog all` from looping forever if a server ever ignored
/// `offset`. Fifty full pages is far past any real worklog, so hitting it means
/// the pagination contract broke, not that the task is chatty — hence the note.
const WORKLOG_MAX_PAGES: usize = 50;

/// One page of `worklogEntries` for a task, newest first.
fn worklog_page(
    client: &Client,
    task_id: &str,
    limit: i64,
    offset: i64,
) -> Result<RunResult<task_worklog::ResponseData>, ClientError> {
    client.run::<TaskWorklog>(task_worklog::Variables {
        filter: Some(task_worklog::WorklogEntryFilterInput {
            task_ids: Some(vec![task_id.to_string()]),
            recurrence_id: None,
            from: None,
            to: None,
            limit: Some(limit),
            offset: Some(offset),
        }),
    })
}

/// Pull the raw `worklogEntries` array out of a response's `data` block.
fn worklog_rows(raw: &serde_json::Value) -> Vec<serde_json::Value> {
    raw.get("worklogEntries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Read as much of one task's worklog as `amount` asks for.
///
/// [`WorklogAmount::Tail`] is a single request for `n + 1` rows: the extra one
/// is how `older_exist` is known without a count query.
///
/// [`WorklogAmount::All`] pages instead, because the server caps any single
/// request at [`WORKLOG_MAX_PAGE`] — asking for `i64::MAX` would quietly return
/// the first thousand and *look* complete, which is the failure this whole
/// branch exists to avoid.
fn fetch_worklog(
    client: &Client,
    task_id: &str,
    amount: WorklogAmount,
) -> Result<WorklogTail, ClientError> {
    let (mut entries, mut raw, older_exist) = match amount {
        // The caller does not fetch at all in this case; answering with an
        // empty page keeps the function total rather than panicking.
        WorklogAmount::None => (Vec::new(), Vec::new(), false),

        WorklogAmount::Tail(n) => {
            let keep = n as usize;
            let r = worklog_page(client, task_id, i64::from(n).saturating_add(1), 0)?;
            let older_exist = r.data.worklog_entries.len() > keep;

            // The server returns newest first (`logged_at DESC`): truncate on
            // that end to keep the *most recent* n.
            let mut entries = r.data.worklog_entries;
            entries.truncate(keep);
            let mut raw = worklog_rows(&r.raw);
            raw.truncate(keep);
            (entries, raw, older_exist)
        }

        WorklogAmount::All => {
            let mut entries = Vec::new();
            let mut raw = Vec::new();
            for page in 0..WORKLOG_MAX_PAGES {
                let offset = WORKLOG_MAX_PAGE * page as i64;
                let r = worklog_page(client, task_id, WORKLOG_MAX_PAGE, offset)?;
                let full = r.data.worklog_entries.len() as i64 == WORKLOG_MAX_PAGE;
                entries.extend(r.data.worklog_entries);
                raw.extend(worklog_rows(&r.raw));
                if !full {
                    break;
                }
                if page == WORKLOG_MAX_PAGES - 1 {
                    eprintln!(
                        "note: stopped after {} entries; the worklog may be incomplete",
                        entries.len()
                    );
                }
            }
            (entries, raw, false)
        }
    };

    // Reverse once, at the end: every branch above collected newest-first.
    entries.reverse();
    raw.reverse();

    Ok(WorklogTail {
        entries,
        older_exist,
        raw: serde_json::Value::Array(raw),
    })
}

pub fn show(api_url: &str, json: bool, task: &str, worklog: WorklogAmount) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, Some(task)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };

    let result = client.run::<GetTask>(get_task::Variables {
        id: target.id.clone(),
    });
    let r = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::Generic;
        }
    };

    // A second round trip rather than a field on `GetTask`: `TaskGql` carries
    // no worklog, and `--worklog 0` should cost nothing. Best-effort, on the
    // `journal`/`dash` precedent — the task detail below printed fine, and an
    // older server without the operation must not turn `show` into a hard
    // error. The failure is never silent though: a note on stderr (stdout
    // stays machine-clean) and, in `--json`, an explicit `null` so "no
    // entries" and "the read failed" can never be confused.
    let tail = if worklog == WorklogAmount::None {
        None
    } else {
        match fetch_worklog(&client, &target.id, worklog) {
            Ok(t) => Some(Some(t)),
            Err(e) => {
                eprintln!("note: worklog unavailable: {}", e);
                Some(None)
            }
        }
    };

    if json {
        let mut raw = r.raw;
        if let (Some(obj), Some(tail)) = (raw.as_object_mut(), tail.as_ref()) {
            let value = match tail {
                Some(t) => t.raw.clone(),
                None => serde_json::Value::Null,
            };
            obj.insert("worklogEntries".to_string(), value);
        }
        if let Err(e) = print_json(&raw) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }

    match r.data.task {
        None => {
            eprintln!("error: task {} not found", target.id);
            return ExitCode::NotFound;
        }
        Some(t) => {
            let key = t.source_id.as_deref().unwrap_or("—");
            println!("{} — {}", key, t.title);
            println!("status:   {:?}", t.status);
            println!(
                "urgency:  {:?}  impact: {:?}  quadrant: {:?}",
                t.urgency, t.impact, t.quadrant
            );
            println!("triage:   {:?}", t.tracking_state);
            if let Some(d) = t.deadline {
                println!("deadline: {}", d);
            }
            if let Some(h) = t.estimated_hours {
                println!("estimate: {}h", h);
            }
            if let Some(desc) = t.description.as_deref() {
                println!("\ndescription:\n{}", desc);
            }
            if let Some(notes) = t.notes.as_deref() {
                println!("\nnotes:\n{}", notes);
            }
        }
    }

    if let Some(Some(tail)) = &tail {
        println!();
        if tail.entries.is_empty() {
            println!("worklog: (empty)");
        } else {
            println!("worklog:");
            if tail.older_exist {
                println!("  \u{2026} older entries not shown (--worklog N)");
            }
            for entry in &tail.entries {
                println!(
                    "{}",
                    format_worklog_entry(&entry.logged_at, &entry.body, &entry.session_id)
                );
            }
        }
    }

    ExitCode::Success
}

pub fn ls(api_url: &str, json: bool, status: &[StatusArg], triage: &[TriageArg]) -> ExitCode {
    let client = Client::new(api_url.to_string());

    // Build filter. If user passed nothing, apply the default: followed only,
    // status not done. If they passed any explicit filter, respect it verbatim.
    let filter = if status.is_empty() && triage.is_empty() {
        list_tasks::TaskFilterInput {
            status: Some(vec![
                list_tasks::TaskStatusGql::TODO,
                list_tasks::TaskStatusGql::IN_PROGRESS,
                list_tasks::TaskStatusGql::BLOCKED,
            ]),
            source: None,
            project_id: None,
            assignee: None,
            deadline_before: None,
            deadline_after: None,
            tag_ids: None,
            tracking_state: Some(vec![list_tasks::TrackingStateGql::FOLLOWED]),
            source_id: None,
            title_contains: None,
        }
    } else {
        list_tasks::TaskFilterInput {
            status: if status.is_empty() {
                None
            } else {
                Some(
                    status
                        .iter()
                        .map(|s| match s {
                            StatusArg::Todo => list_tasks::TaskStatusGql::TODO,
                            StatusArg::InProgress => list_tasks::TaskStatusGql::IN_PROGRESS,
                            StatusArg::Done => list_tasks::TaskStatusGql::DONE,
                            StatusArg::Blocked => list_tasks::TaskStatusGql::BLOCKED,
                        })
                        .collect(),
                )
            },
            source: None,
            project_id: None,
            assignee: None,
            deadline_before: None,
            deadline_after: None,
            tag_ids: None,
            tracking_state: if triage.is_empty() {
                None
            } else {
                Some(
                    triage
                        .iter()
                        .map(|t| match t {
                            TriageArg::Inbox => list_tasks::TrackingStateGql::INBOX,
                            TriageArg::Followed => list_tasks::TrackingStateGql::FOLLOWED,
                            TriageArg::Dismissed => list_tasks::TrackingStateGql::DISMISSED,
                        })
                        .collect(),
                )
            },
            source_id: None,
            title_contains: None,
        }
    };

    let result = client.run::<ListTasks>(list_tasks::Variables {
        filter: Some(filter),
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
            for edge in &r.data.tasks.edges {
                let n = &edge.node;
                let key = n.source_id.as_deref().unwrap_or("—");
                println!(
                    "{:10} {:14} {:8} {}",
                    key,
                    format!("{:?}", n.status),
                    format!("{:?}", n.urgency),
                    n.title
                );
            }
            println!("({} task(s))", r.data.tasks.total_count);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn current(api_url: &str, json: bool, session: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let active = active_task_id(&client);
    if json {
        // Additive only: `.currentActivity` still names the global pointer, unchanged
        // — `aplan-session-start.sh` / `aplan-session-end.sh` read `.currentActivity.task.id`
        // today, and plan 3 owns rewriting them. `actor` just says who asked.
        let actor = session.unwrap_or("manual");
        let mut payload = match &active {
            Some(id) => match resolve_task(&client, Some(id)) {
                Ok(t) => serde_json::json!({ "currentActivity": { "task": { "id": t.id, "title": t.title, "sourceId": t.source_id } } }),
                Err(_) => serde_json::json!({ "currentActivity": null }),
            },
            None => serde_json::json!({ "currentActivity": null }),
        };
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("actor".to_string(), serde_json::json!(actor));
        }
        if let Err(e) = print_json(&payload) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }
    match active {
        Some(id) => match resolve_task(&client, Some(&id)) {
            Ok(t) => println!("▶ tracking: {}", t.title),
            Err(_) => println!("▶ tracking task {}", id),
        },
        None => println!("(no task being tracked)"),
    }
    ExitCode::Success
}

pub fn priority(
    api_url: &str,
    json: bool,
    task: &str,
    urgency: Option<&UrgencyArg>,
    impact: Option<&ImpactArg>,
    reset: bool,
) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, Some(task)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };

    if reset {
        let result = client.run::<ResetUrgency>(reset_urgency::Variables {
            task_id: target.id.clone(),
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
                let label = r
                    .data
                    .reset_urgency
                    .source_id
                    .as_deref()
                    .unwrap_or(&r.data.reset_urgency.title);
                println!(
                    "↺ {}: urgency reset to auto ({:?})",
                    label, r.data.reset_urgency.urgency
                );
                return ExitCode::Success;
            }
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::Generic;
            }
        }
    }

    if urgency.is_none() && impact.is_none() {
        eprintln!("error: provide --urgency, --impact, or --reset");
        return ExitCode::PreconditionFailed;
    }

    let result = client.run::<UpdatePriority>(update_priority::Variables {
        task_id: target.id.clone(),
        urgency: urgency.map(|u| match u {
            UrgencyArg::Low => update_priority::UrgencyLevelGql::LOW,
            UrgencyArg::Medium => update_priority::UrgencyLevelGql::MEDIUM,
            UrgencyArg::High => update_priority::UrgencyLevelGql::HIGH,
            UrgencyArg::Critical => update_priority::UrgencyLevelGql::CRITICAL,
        }),
        impact: impact.map(|i| match i {
            ImpactArg::Low => update_priority::ImpactLevelGql::LOW,
            ImpactArg::Medium => update_priority::ImpactLevelGql::MEDIUM,
            ImpactArg::High => update_priority::ImpactLevelGql::HIGH,
            ImpactArg::Critical => update_priority::ImpactLevelGql::CRITICAL,
        }),
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
            let label = r
                .data
                .update_priority
                .source_id
                .as_deref()
                .unwrap_or(&r.data.update_priority.title);
            println!(
                "⚑ {}: urgency={:?} impact={:?}",
                label, r.data.update_priority.urgency, r.data.update_priority.impact
            );
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn rm(api_url: &str, json: bool, task: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, Some(task)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };
    let result = client.run::<DeleteTask>(delete_task::Variables {
        id: target.id.clone(),
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
            if r.data.delete_task {
                println!("✗ deleted {}", target.id);
            } else {
                eprintln!("error: delete returned false");
                return ExitCode::Generic;
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn new(
    api_url: &str,
    json: bool,
    title: &str,
    deadline: Option<&str>,
    urgency: Option<&UrgencyArg>,
    impact: Option<&ImpactArg>,
    hours: Option<f64>,
) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let input = create_task::CreateTaskInput {
        title: title.to_string(),
        description: None,
        notes: None,
        project_id: None,
        deadline: deadline.map(|s| s.to_string()),
        planned_start: None,
        planned_end: None,
        estimated_hours: hours,
        impact: impact.map(|i| match i {
            ImpactArg::Low => create_task::ImpactLevelGql::LOW,
            ImpactArg::Medium => create_task::ImpactLevelGql::MEDIUM,
            ImpactArg::High => create_task::ImpactLevelGql::HIGH,
            ImpactArg::Critical => create_task::ImpactLevelGql::CRITICAL,
        }),
        urgency: urgency.map(|u| match u {
            UrgencyArg::Low => create_task::UrgencyLevelGql::LOW,
            UrgencyArg::Medium => create_task::UrgencyLevelGql::MEDIUM,
            UrgencyArg::High => create_task::UrgencyLevelGql::HIGH,
            UrgencyArg::Critical => create_task::UrgencyLevelGql::CRITICAL,
        }),
        tag_ids: None,
    };

    let result = client.run::<CreateTask>(create_task::Variables { input });
    match result {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {}", e);
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            println!("＋ created: {}", r.data.create_task.title);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn sync(api_url: &str, json: bool, source: Option<&SourceArg>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let result = client.run::<ForceSync>(force_sync::Variables {
        source: source.map(|s| match s {
            SourceArg::Jira => force_sync::SourceGql::JIRA,
            SourceArg::Excel => force_sync::SourceGql::EXCEL,
            SourceArg::Outlook => force_sync::SourceGql::OUTLOOK,
            SourceArg::Obsidian => force_sync::SourceGql::OBSIDIAN,
            SourceArg::Personal => force_sync::SourceGql::PERSONAL,
            SourceArg::Gryzzly => force_sync::SourceGql::GRYZZLY,
        }),
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
            for s in &r.data.force_sync {
                println!(
                    "{:?}: {:?}  (last: {})",
                    s.source,
                    s.status,
                    s.last_sync_at.as_deref().unwrap_or("never")
                );
                if let Some(err) = s.error_message.as_deref() {
                    println!("  error: {}", err);
                }
            }
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn resolve(api_url: &str, json: bool, alert: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let result = client.run::<ResolveAlert>(resolve_alert::Variables {
        id: alert.to_string(),
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
            println!("✓ resolved alert {}", r.data.resolve_alert.id);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn flush(api_url: &str, json: bool, task: &str, session: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, Some(task)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };
    // With a session, this carries *that* session's window and advances its
    // own `last_flush_at` server-side; without one, it is the human's own
    // window, exactly as before.
    let sid = present_session(session);
    match client.run::<FlushWorklogTime>(flush_worklog_time::Variables {
        task_id: target.id.clone(),
        session_id: sid.map(|s| s.to_string()),
    }) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {}", e);
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            println!("⤓ {}: worklog time flushed", target.title);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn config(api_url: &str, json: bool, cmd: &ConfigCmd) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match cmd {
        ConfigCmd::Get { key } => {
            let result = client.run::<GetConfiguration>(get_configuration::Variables {});
            match result {
                Ok(r) => {
                    if json {
                        if let Err(e) = print_json(&r.raw) {
                            eprintln!("error writing output: {}", e);
                            return ExitCode::Generic;
                        }
                        return ExitCode::Success;
                    }
                    let cfg = &r.data.configuration;
                    match key {
                        Some(k) => {
                            if let Some(v) = cfg.get(k.as_str()) {
                                println!("{} = {}", k, v);
                            } else {
                                eprintln!("error: no such config key `{}`", k);
                                return ExitCode::NotFound;
                            }
                        }
                        None => {
                            if let Some(map) = cfg.as_object() {
                                for (k, v) in map {
                                    println!("{} = {}", k, v);
                                }
                            }
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    ExitCode::Generic
                }
            }
        }
        ConfigCmd::Set { key, value } => {
            let result = client.run::<UpdateConfiguration>(update_configuration::Variables {
                key: key.clone(),
                value: value.clone(),
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
                    let _ = r.data.update_configuration;
                    println!("✓ {} = {}", key, value);
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

#[cfg(test)]
mod at_parsing_tests {
    use super::*;
    use chrono::NaiveDate;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 11).unwrap()
    }

    fn parsed(value: &str) -> String {
        parse_at(value, today())
            .unwrap_or_else(|e| panic!("{value} should parse: {e}"))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }

    #[test]
    fn a_date_and_time_is_taken_as_written() {
        assert_eq!(parsed("2026-08-06T14:30"), "2026-08-06 14:30:00");
        assert_eq!(parsed("2026-08-06T14:30:09"), "2026-08-06 14:30:09");
        assert_eq!(parsed("2026-08-06 14:30"), "2026-08-06 14:30:00");
        assert_eq!(parsed("2026-08-06 14:30:09"), "2026-08-06 14:30:09");
    }

    #[test]
    fn a_bare_time_means_today() {
        assert_eq!(parsed("14:30"), "2026-08-11 14:30:00");
        assert_eq!(parsed("09:05:30"), "2026-08-11 09:05:30");
    }

    /// The documented default, and load-bearing: a worklog entry is evidence for the
    /// 45 minutes before it, clipped to the working windows, so midnight (no window)
    /// and 08:00 (its shadow falls before the window opens) both bill nothing. Noon
    /// carries a full 45 minutes inside the morning.
    #[test]
    fn a_bare_date_lands_at_noon() {
        assert_eq!(parsed("2026-08-06"), "2026-08-06 12:00:00");
    }

    #[test]
    fn a_bare_time_is_not_mistaken_for_a_date() {
        // `14:30` must reach the time-only branch rather than being refused by the
        // date branch that runs before it.
        assert_eq!(parse_at("14:30", today()).unwrap().date(), today());
    }

    #[test]
    fn a_moment_that_is_not_one_is_refused_with_the_accepted_forms() {
        for bogus in ["hier", "06/08/2026", "2026-13-40", "2026-08-06T25:00", "", "14h30"] {
            let error = parse_at(bogus, today())
                .expect_err(&format!("{bogus} must not parse"));
            assert!(error.contains("YYYY-MM-DDTHH:MM"), "{bogus}: {error}");
        }
    }

    #[test]
    fn the_half_day_line_names_both_halves_when_both_were_rebuilt() {
        use rebuild_worklog_projection::HalfDayGql;
        assert_eq!(half_day_labels(&[HalfDayGql::MORNING]), "morning");
        assert_eq!(half_day_labels(&[HalfDayGql::AFTERNOON]), "afternoon");
        assert_eq!(
            half_day_labels(&[HalfDayGql::MORNING, HalfDayGql::AFTERNOON]),
            "morning+afternoon"
        );
    }
}

#[cfg(test)]
mod overlap_display_tests {
    use super::*;

    #[test]
    fn manuel_is_printed_for_the_human_never_an_empty_parenthesis() {
        assert_eq!(overlap_actor_label(&None), "manuel");
    }

    #[test]
    fn a_session_id_is_shortened_to_its_first_four_characters() {
        assert_eq!(
            overlap_actor_label(&Some("a1b2c3d4-ffff".to_string())),
            "a1b2"
        );
    }

    #[test]
    fn a_session_id_shorter_than_four_characters_is_kept_whole() {
        assert_eq!(overlap_actor_label(&Some("ab".to_string())), "ab");
    }

    /// Pins the spec's exact string. A wrong separator, a translated word, or
    /// a dropped `⚠` would all fail this — not merely "prints something".
    #[test]
    fn the_journal_line_matches_the_pinned_spec_string() {
        let line = format_overlap_line(
            47,
            "Saft cadrage",
            "Cartier",
            &Some("a1b2c3".to_string()),
            &None,
        );
        assert_eq!(
            line,
            "\u{26a0} recouvrement 47 min \u{2014} Saft cadrage \u{2194} Cartier (session a1b2 \u{2194} manuel)"
        );
    }

    /// The case task 8's review flagged as reachable and easy to get wrong:
    /// two manual slots overlapping. Neither side may render as an empty
    /// parenthesis.
    #[test]
    fn both_sides_manual_prints_manuel_twice() {
        let line = format_overlap_line(10, "A", "B", &None, &None);
        assert!(line.ends_with("(session manuel \u{2194} manuel)"), "{line}");
    }

    #[test]
    fn a_single_overlap_is_not_pluralised() {
        let line = format_dash_overlap_summary(1, 47);
        assert!(line.contains("1 recouvrement "), "{line}");
        assert!(!line.contains("recouvrements"), "{line}");
    }

    /// Pins the spec's exact dash string, including the plural agreement a
    /// hardcoded "s" or a hardcoded singular would both fail.
    #[test]
    fn the_dash_line_matches_the_pinned_spec_string() {
        let line = format_dash_overlap_summary(2, 50);
        assert_eq!(
            line,
            "\u{26a0} 2 recouvrements aujourd'hui (50 min au total) \u{2014} d\u{e9}tail : aplan journal"
        );
    }
}

#[cfg(test)]
mod worklog_display_tests {
    use super::*;

    /// A timestamp with no offset is not RFC 3339, so this exercises the
    /// fallback — and with it, the only rendering that is the same in every
    /// timezone, which is what lets the layout below be pinned exactly.
    #[test]
    fn an_unparseable_timestamp_keeps_its_first_sixteen_characters() {
        assert_eq!(worklog_stamp("2026-08-12T09:12:00"), "2026-08-12 09:12");
        assert_eq!(worklog_stamp("nonsense"), "nonsense");
    }

    /// The stored instant is UTC; the reader thinks in the wall clock
    /// `aplan log --at` accepts. Computed here rather than hardcoded so the
    /// test states the rule instead of the tester's timezone.
    #[test]
    fn a_stored_utc_instant_is_printed_in_local_time() {
        let expected = chrono::DateTime::parse_from_rfc3339("2026-08-12T07:12:00Z")
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string();
        assert_eq!(worklog_stamp("2026-08-12T07:12:00Z"), expected);
    }

    #[test]
    fn a_human_entry_prints_the_manuel_column_padded_to_the_session_width() {
        assert_eq!(
            format_worklog_entry("2026-08-12T09:12:00", "Relu le plan de migration.", &None),
            "  2026-08-12 09:12  manuel  Relu le plan de migration."
        );
    }

    /// The point of the column: a session-written entry and a hand-written one
    /// must line their bodies up, or the tail is unreadable.
    #[test]
    fn a_session_entry_aligns_its_body_with_a_human_one() {
        let session = format_worklog_entry(
            "2026-08-12T09:12:00",
            "Root cause identified.",
            &Some("a1b2c3d4-ffff".to_string()),
        );
        assert_eq!(
            session,
            "  2026-08-12 09:12  a1b2    Root cause identified."
        );
        let human = format_worklog_entry("2026-08-12T09:12:00", "x", &None);
        assert_eq!(
            session.find("Root").unwrap(),
            human.find('x').unwrap(),
            "session and manual bodies must start in the same column"
        );
    }

    /// An entry is a quotation: a body written over several lines keeps its
    /// breaks, indented under the first line rather than collapsed into it.
    #[test]
    fn a_multi_line_body_indents_its_continuation_under_the_first_line() {
        let line = format_worklog_entry("2026-08-12T09:12:00", "first\nsecond", &None);
        let (head, tail) = line.split_once('\n').expect("two lines");
        assert_eq!(head, "  2026-08-12 09:12  manuel  first");
        assert_eq!(tail, format!("{}second", " ".repeat(head.find("first").unwrap())));
    }
}
