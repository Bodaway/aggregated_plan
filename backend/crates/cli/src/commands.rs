//! Subcommand implementations. Each function takes the parsed `Cli` for global
//! flags (api_url, json) and returns an exit code.

use crate::cli::{StatusArg, TriageArg};
use crate::client::Client;
use crate::lookup::{resolve_task, LookupError};
use crate::output::{print_json, ExitCode};
use crate::queries::{
    append_task_notes, complete_task, current_activity, daily_dashboard, get_task, list_tasks,
    priority_matrix, set_tracking_state, start_activity, stop_activity, update_task_status,
    AppendTaskNotes, CompleteTask, CurrentActivity, DailyDashboard, GetTask, ListTasks,
    PriorityMatrix, SetTrackingState, StartActivity, StopActivity, UpdateTaskStatus,
};

pub fn start(api_url: &str, json: bool, task: &str) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, Some(task)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            return e.exit_code();
        }
    };
    let result = client.run::<StartActivity>(start_activity::Variables {
        task_id: Some(target.id.clone()),
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
            let half_day = format!("{:?}", r.data.start_activity.half_day).to_lowercase();
            let title = r
                .data
                .start_activity
                .task
                .as_ref()
                .map(|t| t.title.as_str())
                .unwrap_or(target.title.as_str());
            println!("▶ started: {} ({} slot)", title, half_day);
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn done(api_url: &str, json: bool, task: Option<&str>, keep_running: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());

    // We need to know whether the running activity matches the target so we can
    // stop the timer iff applicable. Fetch current activity once up front.
    let current = match client.run::<CurrentActivity>(current_activity::Variables {}) {
        Ok(r) => r.data.current_activity,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::Generic;
        }
    };

    let target_id = if let Some(token) = task {
        match resolve_task(&client, Some(token)) {
            Ok(t) => t.id,
            Err(e) => {
                eprintln!("error: {}", e);
                return e.exit_code();
            }
        }
    } else {
        match current.as_ref().and_then(|c| c.task_id.clone()) {
            Some(id) => id,
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

    // Stop the timer iff it was tracking this task and --keep-running not set
    let mut stopped_minutes: Option<i64> = None;
    let should_stop = !keep_running
        && current
            .as_ref()
            .and_then(|c| c.task_id.as_ref())
            .map(|tid| tid == &target_id)
            .unwrap_or(false);

    if should_stop {
        match client.run::<StopActivity>(stop_activity::Variables {}) {
            Ok(r) => stopped_minutes = r.data.stop_activity.and_then(|s| s.duration_minutes),
            Err(e) => {
                eprintln!("warning: failed to stop activity after completing: {}", e);
            }
        }
    }

    if json {
        let completed_json = completed_raw
            .get("completeTask")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let payload = serde_json::json!({
            "completed": completed_json,
            "stoppedMinutes": stopped_minutes,
        });
        if let Err(e) = print_json(&payload) {
            eprintln!("error writing output: {}", e);
            return ExitCode::Generic;
        }
        return ExitCode::Success;
    }

    let label = completed.source_id.as_deref().unwrap_or(&completed.title);
    match stopped_minutes {
        Some(m) => {
            let h = m / 60;
            let mm = m % 60;
            println!("✓ {} done — timer stopped ({}h {}m logged)", label, h, mm);
        }
        None => println!("✓ {} done", label),
    }
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

pub fn status(api_url: &str, json: bool, state: &StatusArg, task: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, task) {
        Ok(t) => t,
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

pub fn note(api_url: &str, json: bool, text: &[String], task: Option<&str>) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let target = match resolve_task(&client, task) {
        Ok(t) => t,
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

pub fn stop(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match client.run::<StopActivity>(stop_activity::Variables {}) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {}", e);
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            match r.data.stop_activity {
                None => println!("(no activity was running)"),
                Some(slot) => {
                    let title = slot
                        .task
                        .as_ref()
                        .map(|t| t.title.as_str())
                        .unwrap_or("(no task)");
                    let mins = slot.duration_minutes.unwrap_or(0);
                    let h = mins / 60;
                    let m = mins % 60;
                    println!("⏹ stopped: {} — {}h {}m logged", title, h, m);
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
    let result = client.run::<DailyDashboard>(daily_dashboard::Variables { date: date_str });
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
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
}

pub fn show(api_url: &str, json: bool, task: &str) -> ExitCode {
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
    match result {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
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
            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::Generic
        }
    }
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

pub fn current(api_url: &str, json: bool) -> ExitCode {
    let client = Client::new(api_url.to_string());
    match client.run::<CurrentActivity>(current_activity::Variables {}) {
        Ok(result) => {
            if json {
                if let Err(e) = print_json(&result.raw) {
                    eprintln!("error writing output: {}", e);
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }
            match result.data.current_activity {
                None => println!("(no activity running)"),
                Some(slot) => {
                    let title = slot
                        .task
                        .as_ref()
                        .map(|t| t.title.as_str())
                        .unwrap_or("(no task)");
                    let half_day = format!("{:?}", slot.half_day).to_lowercase();
                    println!(
                        "▶ {} — started at {} ({})",
                        title, slot.start_time, half_day
                    );
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
