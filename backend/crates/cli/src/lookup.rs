//! Resolve a user-supplied task token (UUID, Jira-shaped key, fuzzy title, or
//! "current") into a concrete task. Pure functions are tested directly; the
//! orchestrating `resolve_task` is exercised in command integration tests.

use crate::client::{Client, ClientError};
use crate::output::ExitCode;
use crate::queries::{
    current_activity, find_task_by_source_id, find_tasks_by_title, CurrentActivity,
    FindTaskBySourceId, FindTasksByTitle,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LookupError {
    #[error("no worklog is currently running\nhint: pass --task <jira-key> to target a specific task,\n      or start one with `aplan start <task>`")]
    NoCurrentActivity,
    #[error("the running activity has no associated task\nhint: pass --task <jira-key> explicitly")]
    CurrentActivityHasNoTask,
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
}

impl LookupError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            LookupError::NoCurrentActivity | LookupError::CurrentActivityHasNoTask => {
                ExitCode::PreconditionFailed
            }
            LookupError::NotFound(_) => ExitCode::NotFound,
            LookupError::Ambiguous { .. } => ExitCode::Ambiguous,
            LookupError::Client(_) => ExitCode::Generic,
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
#[allow(dead_code)]
pub fn resolve_task(client: &Client, token: Option<&str>) -> Result<TaskRef, LookupError> {
    let raw = token.unwrap_or("");
    match classify(raw) {
        TokenShape::Empty | TokenShape::Current => resolve_from_current_activity(client),
        TokenShape::Uuid => Ok(TaskRef {
            id: raw.trim().to_string(),
            title: String::new(),
            source_id: None,
        }),
        TokenShape::SourceIdLike => resolve_by_source_id(client, raw.trim()),
        TokenShape::Fuzzy => resolve_by_title(client, raw.trim()),
    }
}

fn resolve_from_current_activity(client: &Client) -> Result<TaskRef, LookupError> {
    let result = client.run::<CurrentActivity>(current_activity::Variables {})?;
    let slot = result
        .data
        .current_activity
        .ok_or(LookupError::NoCurrentActivity)?;
    let task = slot.task.ok_or(LookupError::CurrentActivityHasNoTask)?;
    Ok(TaskRef {
        id: task.id,
        title: task.title,
        source_id: None,
    })
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
