use clap::{Parser, Subcommand};

#[derive(clap::ValueEnum, Clone, Debug)]
#[value(rename_all = "snake_case")]
pub enum StatusArg {
    Todo,
    InProgress,
    Done,
    Blocked,
}

impl StatusArg {
    pub fn as_graphql(&self) -> crate::queries::update_task_status::TaskStatusGql {
        use crate::queries::update_task_status::TaskStatusGql;
        match self {
            StatusArg::Todo => TaskStatusGql::TODO,
            StatusArg::InProgress => TaskStatusGql::IN_PROGRESS,
            StatusArg::Done => TaskStatusGql::DONE,
            StatusArg::Blocked => TaskStatusGql::BLOCKED,
        }
    }
}

#[derive(clap::ValueEnum, Clone, Debug)]
#[value(rename_all = "snake_case")]
pub enum TriageArg {
    Inbox,
    Followed,
    Dismissed,
}

impl TriageArg {
    pub fn as_graphql(&self) -> crate::queries::set_tracking_state::TrackingStateGql {
        use crate::queries::set_tracking_state::TrackingStateGql;
        match self {
            TriageArg::Inbox => TrackingStateGql::INBOX,
            TriageArg::Followed => TrackingStateGql::FOLLOWED,
            TriageArg::Dismissed => TrackingStateGql::DISMISSED,
        }
    }
}

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
    /// Append a markdown note to the currently-tracked task (or --task TARGET).
    Note {
        /// Note text. Variadic — multiple words are joined with spaces.
        #[arg(required = true)]
        text: Vec<String>,
        /// Override the implicit current-activity target.
        #[arg(long)]
        task: Option<String>,
    },
    /// Set the status of the currently-tracked task (or --task TARGET).
    Status {
        state: StatusArg,
        #[arg(long)]
        task: Option<String>,
    },
    /// Set tracking state on a task. TASK is required.
    Triage {
        state: TriageArg,
        task: String,
    },
    /// Mark TASK done (defaults to currently-tracked) and stop the timer if it
    /// was tracking the same task. Use --keep-running to skip the stop.
    Done {
        /// Optional explicit target.
        task: Option<String>,
        #[arg(long)]
        keep_running: bool,
    },
    /// Show full detail for TASK (UUID, key, fuzzy, or @current).
    Show { task: String },
    /// Daily dashboard summary (tasks, meetings, alerts).
    Dash {
        /// Defaults to today.
        #[arg(long)]
        date: Option<String>,
    },
    /// Print the Eisenhower priority matrix grouped by quadrant.
    Matrix,
    /// List tasks. Default filter: tracking_state=followed, status≠done.
    Ls {
        /// Filter by status. Repeat to allow multiple.
        #[arg(long, value_enum)]
        status: Vec<StatusArg>,
        /// Filter by tracking state. Repeat to allow multiple.
        #[arg(long, value_enum)]
        triage: Vec<TriageArg>,
    },
}
