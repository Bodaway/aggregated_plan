use std::process::ExitCode;

use clap::Parser;

mod cli;
mod client;
mod commands;
mod lookup;
mod output;
mod queries;
mod timesheet_cmd;

fn main() -> ExitCode {
    dotenvy::dotenv().ok();
    let args = cli::Cli::parse();
    let code = match args.command {
        cli::Commands::Version => {
            println!("aplan {}", env!("CARGO_PKG_VERSION"));
            output::ExitCode::Success
        }
        cli::Commands::Current => commands::current(&args.api_url, args.json),
        cli::Commands::Start { task } => commands::start(&args.api_url, args.json, &task),
        cli::Commands::Stop => commands::stop(&args.api_url, args.json),
        cli::Commands::Flush { task } => commands::flush(&args.api_url, args.json, &task),
        cli::Commands::Note { text, task } => {
            commands::note(&args.api_url, args.json, &text, task.as_deref())
        }
        cli::Commands::Log { text, task } => {
            commands::log(&args.api_url, args.json, &text, task.as_deref())
        }
        cli::Commands::Status { state, task } => {
            commands::status(&args.api_url, args.json, &state, task.as_deref())
        }
        cli::Commands::Triage { state, task } => {
            commands::triage(&args.api_url, args.json, &state, &task)
        }
        cli::Commands::Done { task, keep_running } => {
            commands::done(&args.api_url, args.json, task.as_deref(), keep_running)
        }
        cli::Commands::Ls { status, triage } => {
            commands::ls(&args.api_url, args.json, &status, &triage)
        }
        cli::Commands::Show { task } => commands::show(&args.api_url, args.json, &task),
        cli::Commands::Dash { date } => commands::dash(&args.api_url, args.json, date.as_deref()),
        cli::Commands::Matrix => commands::matrix(&args.api_url, args.json),
        cli::Commands::Journal { date } => {
            commands::journal(&args.api_url, args.json, date.as_deref())
        }
        cli::Commands::Alerts { all } => commands::alerts(&args.api_url, args.json, all),
        cli::Commands::Rm { task } => commands::rm(&args.api_url, args.json, &task),
        cli::Commands::Priority {
            task,
            urgency,
            impact,
            reset,
        } => commands::priority(
            &args.api_url,
            args.json,
            &task,
            urgency.as_ref(),
            impact.as_ref(),
            reset,
        ),
        cli::Commands::New {
            title,
            deadline,
            urgency,
            impact,
            hours,
        } => commands::new(
            &args.api_url,
            args.json,
            &title,
            deadline.as_deref(),
            urgency.as_ref(),
            impact.as_ref(),
            hours,
        ),
        cli::Commands::Sync { source } => commands::sync(&args.api_url, args.json, source.as_ref()),
        cli::Commands::Resolve { alert } => commands::resolve(&args.api_url, args.json, &alert),
        cli::Commands::Config { cmd } => commands::config(&args.api_url, args.json, &cmd),
        cli::Commands::Timesheet { date, action } => match action {
            None => timesheet_cmd::timesheet(&args.api_url, args.json, date.as_deref()),
            Some(cli::TimesheetAction::Validate) => {
                timesheet_cmd::timesheet_validate(&args.api_url, args.json, date.as_deref())
            }
            Some(cli::TimesheetAction::Set { project, hours }) => timesheet_cmd::timesheet_set(
                &args.api_url,
                args.json,
                date.as_deref(),
                &project,
                hours,
            ),
            Some(cli::TimesheetAction::Off { am, pm }) => {
                timesheet_cmd::timesheet_off(&args.api_url, args.json, date.as_deref(), am, pm)
            }
        },
        cli::Commands::Map { cmd } => match cmd {
            cli::MapCmd::Add {
                repo,
                branch,
                meeting_subject,
                meeting_organizer,
                internal_project,
                project,
            } => timesheet_cmd::map_add(
                &args.api_url,
                args.json,
                repo.as_deref(),
                branch.as_deref(),
                meeting_subject.as_deref(),
                meeting_organizer.as_deref(),
                internal_project.as_deref(),
                &project,
            ),
            cli::MapCmd::List => timesheet_cmd::map_list(&args.api_url, args.json),
        },
    };
    code.into()
}
