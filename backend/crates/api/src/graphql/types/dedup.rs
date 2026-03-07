use async_graphql::Object;

use super::task::TaskGql;

/// A deduplication suggestion showing two potentially duplicate tasks.
pub struct DeduplicationSuggestionGql {
    pub task_a: TaskGql,
    pub task_b: TaskGql,
    pub confidence_score: f64,
}

#[Object]
impl DeduplicationSuggestionGql {
    /// The first task in the potential duplicate pair.
    async fn task_a(&self) -> &TaskGql {
        &self.task_a
    }

    /// The second task in the potential duplicate pair.
    async fn task_b(&self) -> &TaskGql {
        &self.task_b
    }

    /// Similarity score between 0.0 and 1.0.
    async fn confidence_score(&self) -> f64 {
        self.confidence_score
    }
}
