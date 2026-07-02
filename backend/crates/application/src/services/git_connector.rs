use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::errors::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    pub repo_path: String,
    pub branch: String,
    pub committed_at: DateTime<Utc>,
    pub message: String,
}

/// Reads commit activity from local git repositories. Impl in infrastructure.
#[async_trait]
pub trait GitConnector: Send + Sync {
    /// All commits authored by the current user across `repo_paths` in [from, to).
    async fn commits_between(
        &self,
        repo_paths: &[String],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<GitCommit>, AppError>;
}

/// Parse `git log --pretty=%cI%x1f%s` output (one commit per line, ISO-8601 commit
/// date, unit-separator, subject). Unparseable lines are skipped.
pub fn parse_git_log(repo_path: &str, branch: &str, stdout: &str) -> Vec<GitCommit> {
    stdout
        .lines()
        .filter_map(|line| {
            let (date_s, subject) = line.split_once('\u{1f}')?;
            let committed_at = DateTime::parse_from_rfc3339(date_s.trim())
                .ok()?
                .with_timezone(&Utc);
            Some(GitCommit {
                repo_path: repo_path.to_string(),
                branch: branch.to_string(),
                committed_at,
                message: subject.to_string(),
            })
        })
        .collect()
}

/// Extract an uppercase Jira-style key (e.g. AP-123) from text, if present.
pub fn jira_key_in(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // find a run of A-Z of length >= 2
        if bytes[i].is_ascii_uppercase() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_uppercase() {
                i += 1;
            }
            let letters = i - start;
            if letters >= 2 && i < bytes.len() && bytes[i] == b'-' {
                let dash = i;
                i += 1;
                let dig_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i > dig_start {
                    return Some(text[start..i].to_string());
                }
                i = dash + 1;
            }
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_commits() {
        let out = "2026-06-08T09:15:00+02:00\u{1f}AP-12 fix login\n2026-06-08T14:02:00+02:00\u{1f}refactor";
        let commits = parse_git_log("/repo", "main", out);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].message, "AP-12 fix login");
        assert_eq!(commits[0].repo_path, "/repo");
        assert_eq!(commits[0].branch, "main");
    }

    #[test]
    fn skips_malformed_lines() {
        let out = "garbage line without separator\n2026-06-08T09:15:00+02:00\u{1f}ok";
        let commits = parse_git_log("/repo", "main", out);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].message, "ok");
    }

    #[test]
    fn extracts_jira_key() {
        assert_eq!(jira_key_in("AP-123 do stuff"), Some("AP-123".to_string()));
        assert_eq!(jira_key_in("feat: PROJ-9 thing"), Some("PROJ-9".to_string()));
        assert_eq!(jira_key_in("no key here"), None);
        assert_eq!(jira_key_in("lowercase ab-1"), None);
    }
}
