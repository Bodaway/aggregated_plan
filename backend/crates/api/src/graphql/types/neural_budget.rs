use async_graphql::SimpleObject;

use domain::rules::claude_usage::{ModelUsage, NeuralBudget, ProjectUsage};

/// Claude token consumption over a rolling window, as the HUD's Neural budget
/// panel draws it.
///
/// Two fields deserve their separation. `consumedTokens` is
/// `input + output + cache_creation` and nothing else; `cacheReadTokens` sits
/// beside it because on the real corpus it measures 36x the other three combined,
/// and folding it in would turn a burn gauge into a cache-hit meter. And
/// `declaredCeiling` is typed in by hand — no public API exposes the subscription
/// quota — which is why the panel says so on screen rather than only in a comment.
#[derive(SimpleObject)]
pub struct NeuralBudgetGql {
    pub window_hours: i32,
    pub consumed_tokens: i64,
    pub cache_read_tokens: i64,
    pub declared_ceiling: i64,
    /// Uncapped: a gauge that silently stops at 1.0 hides the one moment it is for.
    pub consumed_ratio: f64,
    /// One total per day, oldest first, most recent last.
    pub per_day: Vec<i64>,
    pub per_model: Vec<ModelUsageGql>,
    pub top_project: Option<ProjectUsageGql>,
}

#[derive(SimpleObject)]
pub struct ModelUsageGql {
    pub model: String,
    pub tokens: i64,
}

#[derive(SimpleObject)]
pub struct ProjectUsageGql {
    /// The absolute path, worktrees folded back onto their checkout. The panel
    /// shows its last segment; the whole path travels so a caller can tell two
    /// same-named directories apart.
    pub name: String,
    pub tokens: i64,
    /// Share of the window's consumption, 0..1.
    pub ratio: f64,
}

impl From<ModelUsage> for ModelUsageGql {
    fn from(usage: ModelUsage) -> Self {
        Self {
            model: usage.model,
            tokens: usage.tokens,
        }
    }
}

impl From<ProjectUsage> for ProjectUsageGql {
    fn from(usage: ProjectUsage) -> Self {
        Self {
            name: usage.name,
            tokens: usage.tokens,
            ratio: usage.ratio,
        }
    }
}

impl From<NeuralBudget> for NeuralBudgetGql {
    fn from(budget: NeuralBudget) -> Self {
        Self {
            window_hours: budget.window_hours as i32,
            consumed_tokens: budget.consumed_tokens,
            cache_read_tokens: budget.cache_read_tokens,
            declared_ceiling: budget.declared_ceiling,
            consumed_ratio: budget.consumed_ratio,
            per_day: budget.per_day,
            per_model: budget.per_model.into_iter().map(Into::into).collect(),
            top_project: budget.top_project.map(Into::into),
        }
    }
}
