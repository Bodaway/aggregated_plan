use async_graphql::Object;

use super::task::TaskGql;

/// The full priority matrix grouped by quadrant.
pub struct PriorityMatrixGql {
    pub urgent_important: Vec<TaskGql>,
    pub important: Vec<TaskGql>,
    pub urgent: Vec<TaskGql>,
    pub neither: Vec<TaskGql>,
}

#[Object]
impl PriorityMatrixGql {
    /// Tasks in the Urgent + Important quadrant (Do First).
    async fn urgent_important(&self) -> &[TaskGql] {
        &self.urgent_important
    }

    /// Tasks in the Important (but not urgent) quadrant (Schedule).
    async fn important(&self) -> &[TaskGql] {
        &self.important
    }

    /// Tasks in the Urgent (but not important) quadrant (Delegate).
    async fn urgent(&self) -> &[TaskGql] {
        &self.urgent
    }

    /// Tasks in the Neither urgent nor important quadrant (Eliminate/Defer).
    async fn neither(&self) -> &[TaskGql] {
        &self.neither
    }
}
