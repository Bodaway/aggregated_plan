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
pub enum UrgencyArg {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(clap::ValueEnum, Clone, Debug)]
#[value(rename_all = "snake_case")]
pub enum ImpactArg {
    Low,
    Medium,
    High,
    Critical,
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
    /// Show the task this session is linked to (the active-task pointer).
    Current,
    /// Link this session to TASK (sets the active-task pointer). Flushes the previously active task's worklog time first.
    Start {
        /// Task to track: UUID, Jira-style key (AP-123), or fuzzy title match.
        task: String,
    },
    /// Flush the active task's worklog time into closed slots and clear the active-task pointer.
    Stop,
    /// Flush the worklog time of TASK into closed activity slots, WITHOUT
    /// clearing the active-task pointer. Used by the SessionEnd hook.
    Flush {
        /// Task to flush: UUID, Jira-style key, or fuzzy title.
        task: String,
    },
    /// Append a markdown note to the currently-tracked task (or --task TARGET).
    Note {
        /// Note text. Variadic — multiple words are joined with spaces.
        #[arg(required = true)]
        text: Vec<String>,
        /// Override the implicit current-activity target.
        #[arg(long)]
        task: Option<String>,
    },
    /// Append a timestamped entry to the worklog of the active task (or --task TARGET).
    Log {
        /// Entry text. Variadic — multiple words are joined with spaces.
        #[arg(required = true)]
        text: Vec<String>,
        /// Override the implicit active-task target.
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
    /// Mark TASK done (defaults to the active task). Flushes its worklog time
    /// and clears the active-task pointer unless --keep-running is set.
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
    /// Print the activity journal for a date (defaults to today).
    Journal {
        #[arg(long)]
        date: Option<String>,
    },
    /// List alerts. Defaults to unresolved only; pass --all for everything.
    Alerts {
        #[arg(long)]
        all: bool,
    },
    /// List tasks. Default filter: tracking_state=followed, status≠done.
    Ls {
        /// Filter by status. Repeat to allow multiple.
        #[arg(long, value_enum)]
        status: Vec<StatusArg>,
        /// Filter by tracking state. Repeat to allow multiple.
        #[arg(long, value_enum)]
        triage: Vec<TriageArg>,
    },
    /// Delete a task.
    Rm { task: String },
    /// Override priority. Provide --urgency and/or --impact, or --reset.
    Priority {
        task: String,
        #[arg(long, value_enum)]
        urgency: Option<UrgencyArg>,
        #[arg(long, value_enum)]
        impact: Option<ImpactArg>,
        #[arg(long, conflicts_with_all = ["urgency", "impact"])]
        reset: bool,
    },
    /// Create a new personal task.
    New {
        title: String,
        #[arg(long)]
        deadline: Option<String>,
        #[arg(long, value_enum)]
        urgency: Option<UrgencyArg>,
        #[arg(long, value_enum)]
        impact: Option<ImpactArg>,
        #[arg(long)]
        hours: Option<f64>,
    },
    /// Trigger a sync. With no --source, syncs all configured sources.
    Sync {
        #[arg(long, value_enum)]
        source: Option<SourceArg>,
    },
    /// Resolve an alert by ID.
    Resolve { alert: String },
    /// Read or write configuration entries.
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Reconstruct + review the day's Gryzzly timesheet (defaults to today).
    Timesheet {
        #[arg(long)]
        date: Option<String>,
        #[command(subcommand)]
        action: Option<TimesheetAction>,
    },
    /// Manage signal→Gryzzly-project mapping rules.
    Map {
        #[command(subcommand)]
        cmd: MapCmd,
    },
    /// Record a memory: a decision, a commitment, a fact or a preference.
    /// Lands in the validation queue unless --confirm is passed.
    Remember {
        /// What is retained, in one sentence.
        title: String,
        #[arg(long, value_enum, default_value = "fact")]
        kind: MemoryKindArg,
        /// The context: why, alternatives dropped. Never a deadline — that lives on the task.
        #[arg(long)]
        why: Option<String>,
        /// Attach to a project: UUID or (fuzzy) name.
        #[arg(long)]
        project: Option<String>,
        /// Person the commitment is towards. Repeat for several.
        #[arg(long = "to")]
        to: Vec<String>,
        /// Attach to a task: UUID, Jira-style key, fuzzy title, or @current.
        #[arg(long)]
        task: Option<String>,
        /// Skip the validation queue and store as active.
        #[arg(long)]
        confirm: bool,
    },
    /// Recall memories: by id, or by search with --q.
    Recall {
        /// Memory id to expand.
        #[arg(required_unless_present = "q", conflicts_with = "q")]
        id: Option<String>,
        /// Free-text search. Jira keys and `Client : subject` labels are safe.
        #[arg(long, short)]
        q: Option<String>,
        /// Include invalidated and not-yet-validated memories.
        /// Search-only: the id path expands one row whatever its status.
        // `conflicts_with = "id"` carries the enforcement: `requires = "q"` alone
        // is waived by clap as soon as `id` — which conflicts with `q` — is
        // present, which is exactly the case where the flag was being ignored.
        #[arg(long, requires = "q", conflicts_with = "id")]
        history: bool,
        /// Restrict the search context to a project: UUID or (fuzzy) name.
        #[arg(long, requires = "q")]
        project: Option<String>,
        /// Max results.
        #[arg(long, default_value_t = 10, requires = "q")]
        limit: i64,
    },
    /// The memory validation queue. With no subcommand, lists pending candidates.
    Inbox {
        #[command(subcommand)]
        cmd: Option<InboxCmd>,
        /// Max candidates to list.
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
    /// Manage stored memories outside the validation queue.
    Memory {
        #[command(subcommand)]
        cmd: MemoryCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum InboxCmd {
    /// Accept a candidate. Refused if it looks like an existing memory, unless --force.
    Accept {
        id: String,
        /// Re-type the candidate on the way in.
        #[arg(long, value_enum)]
        kind: Option<MemoryKindArg>,
        /// Accept despite near-duplicates (an explicit add, never a silent one).
        #[arg(long)]
        force: bool,
    },
    /// Same fact, better wording: fold the candidate into an existing memory.
    /// One row survives — this ERASES history. Use `supersede` if the fact changed.
    Merge {
        id: String,
        /// The active memory that keeps its identity and receives the new wording.
        #[arg(long)]
        into: String,
    },
    /// The fact CHANGED: this candidate replaces an active memory. Both rows
    /// survive; the old one is marked no longer true.
    Supersede {
        id: String,
        /// The active memory this candidate makes obsolete.
        #[arg(long)]
        replaces: String,
    },
    /// Reject a candidate. Kept as a tombstone so it is never re-proposed.
    Reject { id: String },
}

#[derive(Subcommand, Debug)]
pub enum MemoryCmd {
    /// One-shot import of a directory of markdown memory files. Idempotent, and
    /// never writes to the directory.
    Import { dir: String },
    /// Revise an already-active memory: OLD becomes no longer true, replaced by
    /// --by. Both rows survive.
    Supersede {
        old: String,
        #[arg(long)]
        by: String,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
#[value(rename_all = "snake_case")]
pub enum MemoryKindArg {
    Decision,
    Commitment,
    Fact,
    Preference,
}

impl MemoryKindArg {
    pub fn as_graphql(&self) -> crate::queries::remember::MemoryKindGql {
        use crate::queries::remember::MemoryKindGql;
        match self {
            MemoryKindArg::Decision => MemoryKindGql::DECISION,
            MemoryKindArg::Commitment => MemoryKindGql::COMMITMENT,
            MemoryKindArg::Fact => MemoryKindGql::FACT,
            MemoryKindArg::Preference => MemoryKindGql::PREFERENCE,
        }
    }

    /// The codegen mints one enum per operation, so `inbox accept` needs its own.
    pub fn as_graphql_accept(&self) -> crate::queries::inbox_accept::MemoryKindGql {
        use crate::queries::inbox_accept::MemoryKindGql;
        match self {
            MemoryKindArg::Decision => MemoryKindGql::DECISION,
            MemoryKindArg::Commitment => MemoryKindGql::COMMITMENT,
            MemoryKindArg::Fact => MemoryKindGql::FACT,
            MemoryKindArg::Preference => MemoryKindGql::PREFERENCE,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum MapCmd {
    /// Add/update a mapping rule (exactly one of --repo/--meeting-subject/--meeting-organizer/--internal-project).
    Add {
        #[arg(long)]
        repo: Option<String>,
        #[arg(long, requires = "repo")]
        branch: Option<String>,
        #[arg(long)]
        meeting_subject: Option<String>,
        #[arg(long)]
        meeting_organizer: Option<String>,
        #[arg(long)]
        internal_project: Option<String>,
        #[arg(long)]
        project: String,
    },
    /// List enabled mapping rules.
    List,
}

#[derive(Subcommand, Debug)]
pub enum TimesheetAction {
    /// Validate the day's draft (ready to copy into Gryzzly).
    Validate,
    /// Pin a project to an exact number of hours.
    Set { project: String, hours: f64 },
    /// Mark the day (or half-day) off.
    Off {
        #[arg(long, conflicts_with = "pm")]
        am: bool,
        #[arg(long)]
        pm: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
#[value(rename_all = "snake_case")]
pub enum SourceArg {
    Jira,
    Excel,
    Outlook,
    Obsidian,
    Personal,
    Gryzzly,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Print all config (or just one key if KEY is given).
    Get { key: Option<String> },
    /// Set KEY to VALUE.
    Set { key: String, value: String },
}
