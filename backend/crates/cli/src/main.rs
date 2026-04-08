use std::process::ExitCode;

use clap::Parser;

mod cli;
mod client;
mod commands;
mod lookup;
mod output;
mod queries;

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
        cli::Commands::Note { text, task } => {
            commands::note(&args.api_url, args.json, &text, task.as_deref())
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
    };
    code.into()
}
