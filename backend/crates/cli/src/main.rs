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
    };
    code.into()
}
