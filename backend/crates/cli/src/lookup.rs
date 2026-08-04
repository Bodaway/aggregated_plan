//! Resolve a user-supplied task token (UUID, Jira-shaped key, fuzzy title, or
//! "current") into a concrete task. Pure functions are tested directly; the
//! orchestrating `resolve_task` is exercised in command integration tests.

use crate::client::{Client, ClientError};
use crate::output::ExitCode;
use crate::queries::{
    claude_session, find_task_by_source_id, find_tasks_by_title, get_configuration, get_task,
    ClaudeSession, FindTaskBySourceId, FindTasksByTitle, GetConfiguration, GetTask,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LookupError {
    #[error("no worklog is currently running\nhint: pass --task <jira-key> to target a specific task,\n      or start one with `aplan start <task>`")]
    NoCurrentActivity,
    #[error("no task matches `{0}`")]
    NotFound(String),
    #[error("`{query}` matches {count} tasks; please be more specific\n{candidates}")]
    Ambiguous {
        query: String,
        count: usize,
        candidates: String,
    },
    #[error(transparent)]
    Client(#[from] ClientError),
    /// The session exists but the user turned logging off for it. A refusal, never a
    /// fallback: falling back onto the global pointer is exactly how a Claude ends up
    /// reporting work on a task the user declined.
    #[error("session {0} is not tracked — aplan logging is off for this session\nhint: `aplan session bind <task>` to start tracking it")]
    SessionNotTracked(String),
    #[error("session {0} has no task bound\nhint: `aplan session bind <task>`")]
    SessionNoTask(String),
    #[error("session {0} has ended")]
    SessionEnded(String),
    /// An explicit lookup (`aplan session show`) found no row for the id. Note
    /// this is *not* raised by `resolve_from_session`: there, an id with no row
    /// falls through to the global pointer instead — see `resolve_target`'s doc.
    #[error("no session {0} is known to aplan")]
    SessionUnknown(String),
}

impl LookupError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            LookupError::NoCurrentActivity => ExitCode::PreconditionFailed,
            LookupError::NotFound(_) => ExitCode::NotFound,
            LookupError::Ambiguous { .. } => ExitCode::Ambiguous,
            LookupError::Client(_) => ExitCode::Generic,
            // A session that refuses is a precondition the store will not leave,
            // which is what exit 4 means everywhere else in this CLI.
            LookupError::SessionNotTracked(_)
            | LookupError::SessionNoTask(_)
            | LookupError::SessionEnded(_) => ExitCode::PreconditionFailed,
            LookupError::SessionUnknown(_) => ExitCode::NotFound,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskRef {
    pub id: String,
    pub title: String,
    #[allow(dead_code)]
    pub source_id: Option<String>,
}

/// Which of `resolve_target`'s three levels actually produced the task. `log`
/// needs this to decide whether a worklog entry's `sessionId` may be sent: only
/// a target that came *through* the session may carry it — an id that merely
/// happened to be set (and was unknown, or was bypassed by `--task`) must not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedVia {
    Task,
    Session,
    GlobalPointer,
}

/// Token shape: which lookup branch should we take?
#[derive(Debug, PartialEq, Eq)]
pub enum TokenShape {
    Empty,
    Current,
    Uuid,
    SourceIdLike,
    Fuzzy,
}

/// Detect the shape of a user-supplied token.
pub fn classify(token: &str) -> TokenShape {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return TokenShape::Empty;
    }
    if trimmed == "@" || trimmed.eq_ignore_ascii_case("current") {
        return TokenShape::Current;
    }
    if uuid::Uuid::parse_str(trimmed).is_ok() {
        return TokenShape::Uuid;
    }
    if is_source_id_shape(trimmed) {
        return TokenShape::SourceIdLike;
    }
    TokenShape::Fuzzy
}

/// Heuristic: matches Jira-style keys like `AP-123` or `INFRA-42`.
pub fn is_source_id_shape(s: &str) -> bool {
    let dash = match s.find('-') {
        Some(d) if d > 0 && d < s.len() - 1 => d,
        _ => return false,
    };
    let (prefix, rest) = s.split_at(dash);
    let suffix = &rest[1..];
    if !prefix
        .chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
    {
        return false;
    }
    if !prefix
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return false;
    }
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    true
}

/// Resolve a token into a concrete task using the GraphQL client.
pub fn resolve_task(client: &Client, token: Option<&str>) -> Result<TaskRef, LookupError> {
    let raw = token.unwrap_or("");
    match classify(raw) {
        TokenShape::Empty | TokenShape::Current => resolve_from_current_activity(client),
        TokenShape::Uuid => hydrate_by_id(client, raw.trim()),
        TokenShape::SourceIdLike => resolve_by_source_id(client, raw.trim()),
        TokenShape::Fuzzy => resolve_by_title(client, raw.trim()),
    }
}

/// Resolve the task a verb with an implicit target should write to.
///
/// Three levels, in this order:
///   1. `--task` — always wins, and never touches the session.
///   2. the session (`--session`, or `CLAUDE_CODE_SESSION_ID`) — a Claude.
///   3. the global pointer — the human, working by hand.
///
/// A session *known to aplan* refuses rather than falling through to level 3.
/// That refusal is the feature: it is what makes "ne pas tracker" hold for a
/// whole session. An id that names no session at all is not that refusal — it
/// carries no decision to honour — so it falls through to level 3 exactly like
/// an absent `--session` would. `CLAUDE_CODE_SESSION_ID` is exported into every
/// Bash call inside a Claude session, and nothing on this branch creates a
/// session row for it yet, so this is the common case, not an edge case.
///
/// The three-way refusal below mirrors `domain::types::SessionTargetRefusal`. It is
/// restated here rather than shared because this crate deliberately depends on no
/// workspace crate — it talks to the backend over GraphQL like any other client.
pub fn resolve_target(
    client: &Client,
    session: Option<&str>,
    task: Option<&str>,
) -> Result<(TaskRef, ResolvedVia), LookupError> {
    if let Some(token) = task.filter(|t| !t.trim().is_empty()) {
        return resolve_task(client, Some(token)).map(|t| (t, ResolvedVia::Task));
    }
    match session.filter(|s| !s.trim().is_empty()) {
        Some(id) => resolve_from_session(client, id),
        None => resolve_task(client, None).map(|t| (t, ResolvedVia::GlobalPointer)),
    }
}

fn resolve_from_session(client: &Client, id: &str) -> Result<(TaskRef, ResolvedVia), LookupError> {
    let result = client.run::<ClaudeSession>(claude_session::Variables { id: id.to_string() })?;
    let found = match result.data.claude_session {
        // No row named `id`: nothing was decided for it, so there is nothing to
        // honour and nothing to misattribute — fall through to the global pointer
        // exactly as an absent `--session` would. This is deliberately not
        // `SessionUnknown`: that refusal is for `aplan session show`, where the
        // user asked about this id directly and a silent fallback would hide the
        // typo instead of reporting it. And it is why the fallthrough reports
        // `ResolvedVia::GlobalPointer`, not `Session`: the session named here
        // named nothing, so it may not be attributed on the write that follows.
        None => return resolve_task(client, None).map(|t| (t, ResolvedVia::GlobalPointer)),
        Some(s) => s,
    };

    if found.ended_at.is_some() {
        return Err(LookupError::SessionEnded(id.to_string()));
    }
    if !matches!(found.mode, claude_session::SessionModeGql::TRACKING) {
        return Err(LookupError::SessionNotTracked(id.to_string()));
    }
    let task_id = found
        .task_id
        .filter(|t| !t.is_empty())
        .ok_or_else(|| LookupError::SessionNoTask(id.to_string()))?;

    hydrate_by_id(client, &task_id).map(|t| (t, ResolvedVia::Session))
}

/// Fetch a task by its UUID and return a fully hydrated `TaskRef`. A `null`
/// task in the response means the id is unknown (`NotFound`); transport and
/// GraphQL errors propagate as `LookupError::Client`.
fn hydrate_by_id(client: &Client, id: &str) -> Result<TaskRef, LookupError> {
    let result = client.run::<GetTask>(get_task::Variables { id: id.to_string() })?;
    match result.data.task {
        None => Err(LookupError::NotFound(id.to_string())),
        Some(t) => Ok(TaskRef {
            id: t.id,
            title: t.title,
            source_id: t.source_id,
        }),
    }
}

fn resolve_from_current_activity(client: &Client) -> Result<TaskRef, LookupError> {
    let result = client.run::<GetConfiguration>(get_configuration::Variables {})?;
    let task_id = result
        .data
        .configuration
        .get("aplan.active_task_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or(LookupError::NoCurrentActivity)?;
    hydrate_by_id(client, &task_id)
}

fn resolve_by_source_id(client: &Client, key: &str) -> Result<TaskRef, LookupError> {
    let result = client.run::<FindTaskBySourceId>(find_task_by_source_id::Variables {
        source_id: key.to_string(),
    })?;
    let mut nodes: Vec<_> = result.data.tasks.edges.into_iter().map(|e| e.node).collect();
    match nodes.len() {
        0 => Err(LookupError::NotFound(key.to_string())),
        1 => {
            let n = nodes.remove(0);
            Ok(TaskRef {
                id: n.id,
                title: n.title,
                source_id: n.source_id,
            })
        }
        n => Err(LookupError::Ambiguous {
            query: key.to_string(),
            count: n,
            candidates: format_source_id_candidates(&nodes),
        }),
    }
}

fn resolve_by_title(client: &Client, needle: &str) -> Result<TaskRef, LookupError> {
    let result = client.run::<FindTasksByTitle>(find_tasks_by_title::Variables {
        needle: needle.to_string(),
    })?;
    let mut nodes: Vec<_> = result.data.tasks.edges.into_iter().map(|e| e.node).collect();
    match nodes.len() {
        0 => Err(LookupError::NotFound(needle.to_string())),
        1 => {
            let n = nodes.remove(0);
            Ok(TaskRef {
                id: n.id,
                title: n.title,
                source_id: n.source_id,
            })
        }
        n => Err(LookupError::Ambiguous {
            query: needle.to_string(),
            count: n,
            candidates: format_title_candidates(&nodes),
        }),
    }
}

fn format_source_id_candidates(
    nodes: &[find_task_by_source_id::FindTaskBySourceIdTasksEdgesNode],
) -> String {
    nodes
        .iter()
        .take(5)
        .map(|n| {
            format!(
                "  - {} {}",
                n.source_id.as_deref().unwrap_or("—"),
                n.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_title_candidates(
    nodes: &[find_tasks_by_title::FindTasksByTitleTasksEdgesNode],
) -> String {
    nodes
        .iter()
        .take(5)
        .map(|n| {
            format!(
                "  - {} {}",
                n.source_id.as_deref().unwrap_or("—"),
                n.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_empty() {
        assert_eq!(classify(""), TokenShape::Empty);
        assert_eq!(classify("   "), TokenShape::Empty);
    }

    #[test]
    fn classify_current_aliases() {
        assert_eq!(classify("@"), TokenShape::Current);
        assert_eq!(classify("current"), TokenShape::Current);
        assert_eq!(classify("CURRENT"), TokenShape::Current);
    }

    #[test]
    fn classify_uuid() {
        assert_eq!(
            classify("00000000-0000-0000-0000-000000000001"),
            TokenShape::Uuid
        );
    }

    #[test]
    fn classify_jira_key() {
        assert_eq!(classify("AP-123"), TokenShape::SourceIdLike);
        assert_eq!(classify("INFRA-42"), TokenShape::SourceIdLike);
        assert_eq!(classify("PROJ2-7"), TokenShape::SourceIdLike);
    }

    #[test]
    fn classify_fuzzy_for_lowercase_or_words() {
        assert_eq!(classify("auth migration"), TokenShape::Fuzzy);
        assert_eq!(classify("ap-123"), TokenShape::Fuzzy);
        assert_eq!(classify("AP-"), TokenShape::Fuzzy);
        assert_eq!(classify("-123"), TokenShape::Fuzzy);
    }

    #[test]
    fn is_source_id_shape_examples() {
        assert!(is_source_id_shape("AP-1"));
        assert!(is_source_id_shape("AP-1234"));
        assert!(is_source_id_shape("INFRA-42"));
        assert!(!is_source_id_shape(""));
        assert!(!is_source_id_shape("AP"));
        assert!(!is_source_id_shape("ap-1"));
        assert!(!is_source_id_shape("AP-"));
        assert!(!is_source_id_shape("AP-1A"));
    }
}
