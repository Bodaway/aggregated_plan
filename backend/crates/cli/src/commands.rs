//! Subcommand implementations. Each function takes the parsed `Cli` for global
//! flags (api_url, json) and returns an exit code.

use crate::client::Client;
use crate::lookup::resolve_task;
use crate::output::{print_json, ExitCode};
use crate::queries::{
    current_activity, start_activity, CurrentActivity, StartActivity,
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
