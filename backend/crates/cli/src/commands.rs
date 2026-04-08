//! Subcommand implementations. Each function takes the parsed `Cli` for global
//! flags (api_url, json) and returns an exit code.

use crate::cli::{StatusArg, TriageArg};
use crate::client::Client;
use crate::lookup::resolve_task;
use crate::output::{print_json, ExitCode};
use crate::queries::{
    append_task_notes, current_activity, set_tracking_state, start_activity, stop_activity,
    update_task_status, AppendTaskNotes, CurrentActivity, SetTrackingState, StartActivity,
    StopActivity, UpdateTaskStatus,
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
