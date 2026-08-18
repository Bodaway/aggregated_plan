//! Compile-time-checked GraphQL operations. Each `GraphQLQuery` derive references
//! a file under `graphql/` and validates it against `graphql/schema.graphql` at
//! build time. Adding a new operation is two steps: write the .graphql file,
//! add a derive here.

use graphql_client::GraphQLQuery;

// Custom scalar mappings used by the codegen.
#[allow(non_camel_case_types)]
type DateTime = String;
#[allow(non_camel_case_types)]
type NaiveDate = String;
#[allow(non_camel_case_types)]
type NaiveDateTime = String;
#[allow(non_camel_case_types, dead_code)]
type ID = String;
#[allow(clippy::upper_case_acronyms)]
type JSON = serde_json::Value;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/health.graphql",
    response_derives = "Debug, Clone"
)]
#[allow(dead_code)]
pub struct Health;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/find_task_by_source_id.graphql",
    response_derives = "Debug, Clone"
)]
pub struct FindTaskBySourceId;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/find_tasks_by_title.graphql",
    response_derives = "Debug, Clone"
)]
pub struct FindTasksByTitle;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/append_task_notes.graphql",
    response_derives = "Debug, Clone"
)]
pub struct AppendTaskNotes;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/update_task_status.graphql",
    response_derives = "Debug, Clone"
)]
pub struct UpdateTaskStatus;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/set_tracking_state.graphql",
    response_derives = "Debug, Clone"
)]
pub struct SetTrackingState;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/complete_task.graphql",
    response_derives = "Debug, Clone"
)]
pub struct CompleteTask;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/list_tasks.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ListTasks;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/get_task.graphql",
    response_derives = "Debug, Clone"
)]
pub struct GetTask;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/daily_dashboard.graphql",
    response_derives = "Debug, Clone"
)]
pub struct DailyDashboard;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/priority_matrix.graphql",
    response_derives = "Debug, Clone"
)]
pub struct PriorityMatrix;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/activity_journal.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ActivityJournal;

/// The day's flagged overlaps (Task 8), consumed by `journal`/`dash`/`timesheet`
/// (Task 9) to show which tasks double-claimed the same stretch of time.
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/activity_overlaps.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ActivityOverlaps;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/list_alerts.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ListAlerts;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/create_task.graphql",
    response_derives = "Debug, Clone"
)]
pub struct CreateTask;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/delete_task.graphql",
    response_derives = "Debug, Clone"
)]
pub struct DeleteTask;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/update_priority.graphql",
    response_derives = "Debug, Clone"
)]
pub struct UpdatePriority;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/reset_urgency.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ResetUrgency;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/force_sync.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ForceSync;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/resolve_alert.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ResolveAlert;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/get_configuration.graphql",
    response_derives = "Debug, Clone"
)]
pub struct GetConfiguration;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/update_configuration.graphql",
    response_derives = "Debug, Clone"
)]
pub struct UpdateConfiguration;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/add_worklog_entry.graphql",
    response_derives = "Debug, Clone"
)]
pub struct AddWorklogEntry;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/flush_worklog_time.graphql",
    response_derives = "Debug, Clone"
)]
pub struct FlushWorklogTime;

/// The read side of `aplan log`: the entries of one task, newest first, as
/// `aplan show` prints them. `AddWorklogEntry` writes, this reads — nothing
/// else in the CLI could get a worklog back out.
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/task_worklog.graphql",
    response_derives = "Debug, Clone"
)]
pub struct TaskWorklog;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/rebuild_worklog_projection.graphql",
    response_derives = "Debug, Clone, PartialEq"
)]
pub struct RebuildWorklogProjection;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/reattribute_worklog.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ReattributeWorklog;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/repair_orphaned_slots.graphql",
    response_derives = "Debug, Clone"
)]
pub struct RepairOrphanedSlots;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/reconstruct_timesheet.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ReconstructTimesheet;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/timesheet_draft.graphql",
    response_derives = "Debug, Clone"
)]
pub struct TimesheetDraft;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/set_quarter_share.graphql",
    response_derives = "Debug, Clone"
)]
pub struct SetQuarterShare;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/validate_timesheet.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ValidateTimesheet;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/mark_day_off.graphql",
    response_derives = "Debug, Clone"
)]
pub struct MarkDayOff;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/learn_mapping.graphql",
    response_derives = "Debug, Clone"
)]
pub struct LearnMapping;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/signal_mappings.graphql",
    response_derives = "Debug, Clone"
)]
pub struct SignalMappings;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/gryzzly_projects.graphql",
    response_derives = "Debug, Clone"
)]
pub struct GryzzlyProjects;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/remember.graphql",
    response_derives = "Debug, Clone"
)]
pub struct Remember;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/get_memory.graphql",
    response_derives = "Debug, Clone"
)]
pub struct GetMemory;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/recall_memories.graphql",
    response_derives = "Debug, Clone"
)]
pub struct RecallMemories;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/list_projects.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ListProjects;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/pending_memories.graphql",
    response_derives = "Debug, Clone"
)]
pub struct PendingMemories;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/inbox_accept.graphql",
    response_derives = "Debug, Clone"
)]
pub struct InboxAccept;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/inbox_reject.graphql",
    response_derives = "Debug, Clone"
)]
pub struct InboxReject;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/inbox_merge.graphql",
    response_derives = "Debug, Clone"
)]
pub struct InboxMerge;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/memory_supersede.graphql",
    response_derives = "Debug, Clone"
)]
pub struct MemorySupersede;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/memory_import.graphql",
    response_derives = "Debug, Clone"
)]
pub struct MemoryImport;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/brief.graphql",
    response_derives = "Debug, Clone"
)]
pub struct Brief;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/search.graphql",
    response_derives = "Debug, Clone"
)]
pub struct Search;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/unconsolidated_entries.graphql",
    response_derives = "Debug, Clone"
)]
pub struct UnconsolidatedEntries;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/mark_consolidated.graphql",
    response_derives = "Debug, Clone"
)]
pub struct MarkConsolidated;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/record_consolidation_run.graphql",
    response_derives = "Debug, Clone"
)]
pub struct RecordConsolidationRun;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/claude_session.graphql",
    response_derives = "Debug, Clone"
)]
pub struct ClaudeSession;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/open_claude_sessions.graphql",
    response_derives = "Debug, Clone"
)]
pub struct OpenClaudeSessions;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/bind_session.graphql",
    response_derives = "Debug, Clone"
)]
pub struct BindSession;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/set_session_mode.graphql",
    response_derives = "Debug, Clone"
)]
pub struct SetSessionMode;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/end_session.graphql",
    response_derives = "Debug, Clone"
)]
pub struct EndSession;
