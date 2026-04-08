use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "aplan", version, about = "Aggregated Plan command-line cockpit")]
pub struct Cli {
    /// API endpoint (default loopback). Override with --api-url or APLAN_API_URL.
    #[arg(
        long,
        env = "APLAN_API_URL",
        default_value = "http://127.0.0.1:3001/graphql",
        global = true
    )]
    pub api_url: String,

    /// Emit machine-readable JSON instead of human-friendly output.
    #[arg(long, global = true)]
    pub json: bool,

    /// Verbose stderr logging (request URL, operation name, elapsed time).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Print the CLI version (smoke test for the scaffold).
    Version,
    /// Show the currently running activity slot, if any.
    Current,
    /// Start a worklog on TASK. Auto-stops any running activity first.
    Start {
        /// Task to track: UUID, Jira-style key (AP-123), or fuzzy title match.
        task: String,
    },
    /// Stop the currently running worklog. Prints duration.
    Stop,
}
