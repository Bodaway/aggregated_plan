//! Subcommand implementations. Each function takes the parsed `Cli` for global
//! flags (api_url, json) and returns an exit code.

use crate::client::Client;
use crate::output::{print_json, ExitCode};
use crate::queries::{current_activity, CurrentActivity};

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
