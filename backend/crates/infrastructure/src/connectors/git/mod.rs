use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::process::Command;

use application::errors::AppError;
use application::services::git_connector::{parse_git_log, GitCommit, GitConnector};

/// Reads commits by shelling out to the local `git` binary. Suitable for a
/// single-user local deployment where the backend can see the user's repos.
pub struct ShellGitConnector;

impl ShellGitConnector {
    pub fn new() -> Self {
        Self
    }

    async fn current_branch(&self, repo_path: &str) -> Option<String> {
        let out = Command::new("git")
            .args(["-C", repo_path, "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .await
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

impl Default for ShellGitConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GitConnector for ShellGitConnector {
    async fn commits_between(
        &self,
        repo_paths: &[String],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<GitCommit>, AppError> {
        let mut all = Vec::new();
        for repo in repo_paths {
            let branch = self.current_branch(repo).await.unwrap_or_else(|| "HEAD".to_string());
            let out = Command::new("git")
                .args([
                    "-C",
                    repo,
                    "log",
                    "--no-merges",
                    &format!("--since={}", from.to_rfc3339()),
                    &format!("--until={}", to.to_rfc3339()),
                    "--pretty=%cI\u{1f}%s",
                ])
                .output()
                .await
                .map_err(|e| AppError::Configuration(format!("git log failed for {repo}: {e}")))?;
            if !out.status.success() {
                // A missing/invalid repo path is non-fatal: skip it.
                continue;
            }
            let stdout = String::from_utf8_lossy(&out.stdout);
            all.extend(parse_git_log(repo, &branch, &stdout));
        }
        Ok(all)
    }
}
