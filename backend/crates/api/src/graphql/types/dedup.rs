use async_graphql::{Object, ID};

use application::use_cases::deduplication::DeduplicationSuggestion;

use super::task::TaskGql;

/// A deduplication suggestion showing two potentially duplicate tasks.
pub struct DeduplicationSuggestionGql {
    pub id: ID,
    pub task_a: TaskGql,
    pub task_b: TaskGql,
    pub confidence_score: f64,
    pub title_similarity: f64,
    pub assignee_match: bool,
    pub project_match: bool,
}

impl From<DeduplicationSuggestion> for DeduplicationSuggestionGql {
    fn from(s: DeduplicationSuggestion) -> Self {
        DeduplicationSuggestionGql {
            id: ID(s.id.to_string()),
            task_a: TaskGql(s.task_a),
            task_b: TaskGql(s.task_b),
            confidence_score: s.confidence_score,
            title_similarity: s.title_similarity,
            assignee_match: s.assignee_match,
            project_match: s.project_match,
        }
    }
}

#[Object]
impl DeduplicationSuggestionGql {
    /// Unique suggestion identifier.
    async fn id(&self) -> &ID {
        &self.id
    }

    /// The first task in the potential duplicate pair.
    async fn task_a(&self) -> &TaskGql {
        &self.task_a
    }

    /// The second task in the potential duplicate pair.
    async fn task_b(&self) -> &TaskGql {
        &self.task_b
    }

    /// Overall similarity score between 0.0 and 1.0.
    async fn confidence_score(&self) -> f64 {
        self.confidence_score
    }

    /// Title similarity score between 0.0 and 1.0.
    async fn title_similarity(&self) -> f64 {
        self.title_similarity
    }

    /// Whether the assignees match.
    async fn assignee_match(&self) -> bool {
        self.assignee_match
    }

    /// Whether the projects match.
    async fn project_match(&self) -> bool {
        self.project_match
    }
}
