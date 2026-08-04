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

    /// The Claude Code session this invocation belongs to. Defaults to
    /// `CLAUDE_CODE_SESSION_ID`, which the harness exports into every Bash call, so
    /// a Claude never has to pass it. Absent (a plain terminal), the global pointer
    /// answers instead: that pointer is the human, working by hand.
    #[arg(long, env = "CLAUDE_CODE_SESSION_ID", global = true)]
    pub session: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Print the CLI version (smoke test for the scaffold).
    Version,
    /// Show the task this session is linked to (the active-task pointer).
    Current,
    /// List the open Claude sessions and what each one is working on.
    Sessions,
    /// Manage this session's aplan link.
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
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
    /// Move logged time from one task to another: the worklog entries AND the
    /// activity slots derived from them. The fix for a day recorded against the
    /// wrong task — that time reaches the timesheet and the client invoice.
    ///
    /// Previews by default and writes only with --confirm, because it rewrites
    /// billing-relevant history: the preview names both tasks and prints the
    /// before/after hours, so the confirmation is informed rather than a reflex.
    Reattribute {
        /// Task the time is wrongly attributed to: UUID, id prefix, Jira-style key,
        /// fuzzy title, or @current.
        #[arg(long)]
        from: String,
        /// Task it belongs to: same forms.
        #[arg(long)]
        to: String,
        /// One local day (YYYY-MM-DD) — the mis-attributed-day case.
        #[arg(long, conflicts_with_all = ["since", "until", "entry"])]
        date: Option<String>,
        /// First local day of a range (inclusive).
        #[arg(long, conflicts_with = "entry")]
        since: Option<String>,
        /// Last local day of a range (inclusive). Defaults to --since.
        #[arg(long, requires = "since", conflicts_with = "entry")]
        until: Option<String>,
        /// A worklog entry to move: full UUID or id prefix. Repeat for several.
        #[arg(long = "entry")]
        entry: Vec<String>,
        /// Apply the move. Without it nothing is written.
        #[arg(long)]
        confirm: bool,
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
        /// Where this came from: a worklog entry id, a session id. Free-form, no
        /// foreign key. The consolidation job records the entry it read here, so a
        /// memory can be traced back to what produced it.
        #[arg(long)]
        source_ref: Option<String>,
        /// The active memory this one CONTRADICTS: full UUID or short reference
        /// (`m:7c1`). Records a supersession *proposal* — nothing is invalidated,
        /// the triage decides. `aplan inbox supersede <id>` then needs no
        /// `--replaces`.
        ///
        /// Refused next to --confirm: a proposal is a question for the queue, and a
        /// confirmed memory never enters it. Revise an established memory with
        /// `aplan memory supersede` instead.
        #[arg(long, conflicts_with = "confirm")]
        contradicts: Option<String>,
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
        /// Search-only, and refused rather than ignored next to an id.
        #[arg(long, requires = "q", conflicts_with = "id")]
        project: Option<String>,
        /// Max results. Search-only, same reason.
        #[arg(long, default_value_t = 10, requires = "q", conflicts_with = "id")]
        limit: i64,
    },
    /// Print the session brief: deadlines, open commitments, active decisions,
    /// the memory queue, and a warning when consolidation has gone quiet.
    /// Capped at 40 lines. It ADDS to the session's task list, never replaces it.
    Brief {
        /// The 08:30 notification variant: today's deadlines, open commitments,
        /// candidates to triage. No decisions, no drill-down hints.
        #[arg(long)]
        morning: bool,
        /// Project in focus: UUID or (fuzzy) name. Defaults to the project of the
        /// task currently tracked.
        #[arg(long)]
        project: Option<String>,
        /// Date to build the brief for (YYYY-MM-DD). Defaults to today.
        #[arg(long)]
        date: Option<String>,
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
    /// Drive the 17:30 consolidation: read the worklog entries nobody has turned
    /// into memories yet, then mark them and record the run.
    ///
    /// The consolidation itself is a scheduled Claude Code session — the backend
    /// holds no model. These verbs are the deterministic half it drives; see
    /// `docs/prompts/consolidation-memoire.md`.
    Consolidate {
        #[command(subcommand)]
        cmd: ConsolidateCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// Show the session's link — what the SessionStart hook reads.
    Show,
    /// Link this session to TASK. Does not move the global pointer.
    Bind {
        task: String,
        /// Displayed in `aplan sessions`. Defaults to the working directory.
        #[arg(long)]
        label: Option<String>,
    },
    /// Disable aplan logging for this session, persistently.
    Off,
    /// Close this session.
    End,
}

#[derive(Subcommand, Debug)]
pub enum ConsolidateCmd {
    /// The worklog entries still awaiting consolidation (`consolidatedAt` null),
    /// oldest first. Read-only: it marks nothing, so it doubles as the
    /// reachability probe a run must pass before doing anything.
    Pending {
        /// Max entries to read in one run.
        #[arg(long, default_value_t = 200)]
        limit: i64,
    },
    /// Mark entries consolidated. Run this LAST, once the memories they produced
    /// are persisted: a duplicate memory is recoverable through the rejection
    /// tombstones, an entry skipped forever is not.
    Mark {
        /// Worklog entry ids (full UUIDs, as `consolidate pending` prints them).
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Record that a consolidation run happened, so `aplan brief` stops reporting
    /// "jamais exécutée" — and starts reporting staleness if the job dies.
    RecordRun,
}

#[derive(Subcommand, Debug)]
pub enum InboxCmd {
    /// Accept a candidate. Refused if it looks like an existing memory, unless --force.
    Accept {
        /// Candidate: full UUID or the short reference displayed (`m:7c1`, `7c1`).
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
        /// Candidate: full UUID or short reference.
        id: String,
        /// The active memory that keeps its identity and receives the new wording:
        /// full UUID or short reference.
        #[arg(long)]
        into: String,
    },
    /// The fact CHANGED: this candidate replaces an active memory. Both rows
    /// survive; the old one is marked no longer true.
    Supersede {
        /// Candidate: full UUID or short reference.
        id: String,
        /// The active memory this candidate makes obsolete: full UUID or short
        /// reference. Optional — defaults to the memory the candidate itself
        /// records as contradicted (what `aplan inbox` shows), which is the case a
        /// consolidation run produces.
        #[arg(long)]
        replaces: Option<String>,
    },
    /// Reject a candidate. Kept as a tombstone so it is never re-proposed.
    Reject {
        /// Candidate: full UUID or short reference.
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum MemoryCmd {
    /// One-shot import of a directory of markdown memory files. Idempotent, and
    /// never writes to the directory.
    Import { dir: String },
    /// Revise an already-active memory: OLD becomes no longer true, replaced by
    /// --by. Both rows survive.
    Supersede {
        /// The memory that is no longer true: full UUID or short reference (`m:7c1`).
        old: String,
        /// The memory that replaces it: full UUID or short reference.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args)
    }

    /// `requires = "q"` alone is waived by clap as soon as the conflicting `id` is
    /// present — so a search-only flag typed next to an id used to be **silently
    /// discarded**. Every one of them must now be refused at parse time.
    #[test]
    fn a_search_only_flag_next_to_an_id_is_refused_not_ignored() {
        for flag in [
            vec!["--history"],
            vec!["--project", "pernod"],
            vec!["--limit", "3"],
        ] {
            let mut args = vec!["aplan", "recall", "7c1"];
            args.extend_from_slice(&flag);
            let err = parse(&args)
                .expect_err(&format!("{flag:?} must not be accepted beside an id"));
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::ArgumentConflict,
                "{flag:?} gave {:?}",
                err.kind()
            );
        }
    }

    /// The conflict must not fire on the *default* value of `--limit`, or the
    /// plain id form would stop working altogether.
    #[test]
    fn expanding_one_memory_by_id_still_parses() {
        let cli = parse(&["aplan", "recall", "m:7c1"]).expect("the id form must parse");
        match cli.command {
            Commands::Recall {
                id,
                q,
                history,
                project,
                limit,
            } => {
                assert_eq!(id.as_deref(), Some("m:7c1"));
                assert_eq!(q, None);
                assert!(!history);
                assert_eq!(project, None);
                assert_eq!(limit, 10, "the default is still applied");
            }
            other => panic!("expected Recall, got {other:?}"),
        }
    }

    #[test]
    fn every_search_flag_is_accepted_next_to_a_query() {
        let cli = parse(&[
            "aplan", "recall", "--q", "engagements", "--history", "--project", "pernod", "--limit",
            "3",
        ])
        .expect("the search form must parse");
        match cli.command {
            Commands::Recall {
                id,
                q,
                history,
                project,
                limit,
            } => {
                assert_eq!(id, None);
                assert_eq!(q.as_deref(), Some("engagements"));
                assert!(history);
                assert_eq!(project.as_deref(), Some("pernod"));
                assert_eq!(limit, 3);
            }
            other => panic!("expected Recall, got {other:?}"),
        }
    }

    #[test]
    fn the_brief_defaults_to_the_session_variant() {
        match parse(&["aplan", "brief"]).expect("parses").command {
            Commands::Brief {
                morning,
                project,
                date,
            } => {
                assert!(!morning);
                assert_eq!(project, None);
                assert_eq!(date, None);
            }
            other => panic!("expected Brief, got {other:?}"),
        }
    }

    /// The consolidation records which worklog entry produced a memory, so a
    /// candidate can be traced back to what it was extracted from (§5.2:
    /// `source_ref` holds a worklog entry id).
    #[test]
    fn remember_carries_the_provenance_of_the_entry_it_came_from() {
        match parse(&[
            "aplan",
            "remember",
            "Wave 0 limitee au perimetre AI Microsoft",
            "--kind",
            "decision",
            "--source-ref",
            "509a006c-0000-0000-0000-000000000001",
        ])
        .expect("parses")
        .command
        {
            Commands::Remember {
                source_ref,
                confirm,
                ..
            } => {
                assert_eq!(
                    source_ref.as_deref(),
                    Some("509a006c-0000-0000-0000-000000000001")
                );
                assert!(!confirm, "the consolidation never writes straight to active");
            }
            other => panic!("expected Remember, got {other:?}"),
        }
    }

    /// The batch default is the same 200 the resolver applies, so the scheduled
    /// job reads the same page whether or not it passes `--limit`.
    #[test]
    fn consolidate_pending_defaults_to_the_batch_limit() {
        match parse(&["aplan", "consolidate", "pending"])
            .expect("parses")
            .command
        {
            Commands::Consolidate {
                cmd: ConsolidateCmd::Pending { limit },
            } => assert_eq!(limit, 200),
            other => panic!("expected Consolidate/Pending, got {other:?}"),
        }
    }

    #[test]
    fn consolidate_mark_takes_several_ids() {
        match parse(&["aplan", "consolidate", "mark", "aaa", "bbb", "ccc"])
            .expect("parses")
            .command
        {
            Commands::Consolidate {
                cmd: ConsolidateCmd::Mark { ids },
            } => assert_eq!(ids, vec!["aaa", "bbb", "ccc"]),
            other => panic!("expected Consolidate/Mark, got {other:?}"),
        }
    }

    /// Marking nothing must be a parse error, not a silent success: a job that
    /// forgot to collect its ids would otherwise look like a clean run and record
    /// a consolidation that consolidated nothing.
    #[test]
    fn consolidate_mark_refuses_an_empty_id_list() {
        let err = parse(&["aplan", "consolidate", "mark"]).expect_err("must require ids");
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::MissingRequiredArgument,
            "got {:?}",
            err.kind()
        );
    }

    /// The real invocation this verb exists for: a whole day on the wrong task.
    /// It must parse WITHOUT `--confirm`, and carry `confirm: false`, or the safe
    /// default is not a default at all.
    #[test]
    fn reattributing_a_day_parses_and_defaults_to_a_preview() {
        match parse(&[
            "aplan",
            "reattribute",
            "--from",
            "b6a62457",
            "--to",
            "35d79540",
            "--date",
            "2026-08-03",
        ])
        .expect("parses")
        .command
        {
            Commands::Reattribute {
                from,
                to,
                date,
                since,
                until,
                entry,
                confirm,
            } => {
                assert_eq!(from, "b6a62457");
                assert_eq!(to, "35d79540");
                assert_eq!(date.as_deref(), Some("2026-08-03"));
                assert_eq!(since, None);
                assert_eq!(until, None);
                assert!(entry.is_empty());
                assert!(!confirm, "the default must write nothing");
            }
            other => panic!("expected Reattribute, got {other:?}"),
        }
    }

    #[test]
    fn reattributing_a_range_and_several_entries_parse() {
        match parse(&[
            "aplan",
            "reattribute",
            "--from",
            "AP-1",
            "--to",
            "AP-2",
            "--since",
            "2026-08-01",
            "--until",
            "2026-08-03",
            "--confirm",
        ])
        .expect("parses")
        .command
        {
            Commands::Reattribute {
                since,
                until,
                confirm,
                ..
            } => {
                assert_eq!(since.as_deref(), Some("2026-08-01"));
                assert_eq!(until.as_deref(), Some("2026-08-03"));
                assert!(confirm);
            }
            other => panic!("expected Reattribute, got {other:?}"),
        }
        match parse(&[
            "aplan",
            "reattribute",
            "--from",
            "AP-1",
            "--to",
            "AP-2",
            "--entry",
            "7c1",
            "--entry",
            "9ab",
        ])
        .expect("parses")
        .command
        {
            Commands::Reattribute { entry, .. } => assert_eq!(entry, vec!["7c1", "9ab"]),
            other => panic!("expected Reattribute, got {other:?}"),
        }
    }

    /// Two selections at once would leave it unclear what was corrected, and the
    /// answer must come at parse time rather than after a round trip.
    #[test]
    fn a_day_and_an_entry_together_are_refused_at_parse_time() {
        for extra in [
            vec!["--date", "2026-08-03", "--entry", "7c1"],
            vec!["--since", "2026-08-01", "--entry", "7c1"],
            vec!["--date", "2026-08-03", "--since", "2026-08-01"],
        ] {
            let mut args = vec!["aplan", "reattribute", "--from", "AP-1", "--to", "AP-2"];
            args.extend_from_slice(&extra);
            let err = parse(&args).expect_err(&format!("{extra:?} must be refused"));
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::ArgumentConflict,
                "{extra:?} gave {:?}",
                err.kind()
            );
        }
    }

    /// `--until` alone would silently move a single day, or nothing at all.
    #[test]
    fn until_requires_since() {
        let err = parse(&[
            "aplan",
            "reattribute",
            "--from",
            "AP-1",
            "--to",
            "AP-2",
            "--until",
            "2026-08-03",
        ])
        .expect_err("must require --since");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn reattribute_requires_both_tasks() {
        for args in [
            vec!["aplan", "reattribute", "--to", "AP-2", "--date", "2026-08-03"],
            vec!["aplan", "reattribute", "--from", "AP-1", "--date", "2026-08-03"],
        ] {
            let err = parse(&args).expect_err("both tasks are required");
            assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
        }
    }

    #[test]
    fn consolidate_record_run_parses() {
        match parse(&["aplan", "consolidate", "record-run"])
            .expect("parses")
            .command
        {
            Commands::Consolidate {
                cmd: ConsolidateCmd::RecordRun,
            } => {}
            other => panic!("expected Consolidate/RecordRun, got {other:?}"),
        }
    }

    #[test]
    fn the_morning_brief_takes_a_project_and_a_date() {
        match parse(&[
            "aplan",
            "brief",
            "--morning",
            "--project",
            "pernod",
            "--date",
            "2026-08-03",
        ])
        .expect("parses")
        .command
        {
            Commands::Brief {
                morning,
                project,
                date,
            } => {
                assert!(morning);
                assert_eq!(project.as_deref(), Some("pernod"));
                assert_eq!(date.as_deref(), Some("2026-08-03"));
            }
            other => panic!("expected Brief, got {other:?}"),
        }
    }
}
