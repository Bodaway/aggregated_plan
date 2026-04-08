use clap::Parser;

mod cli;
mod queries;

fn main() {
    dotenvy::dotenv().ok();
    let args = cli::Cli::parse();
    match args.command {
        cli::Commands::Version => {
            println!("aplan {}", env!("CARGO_PKG_VERSION"));
            // Sanity: codegen produced a Health type. Reference it so the
            // compiler proves the GraphQL schema parsed.
            let _ = std::mem::size_of::<queries::health::ResponseData>();
        }
    }
}
