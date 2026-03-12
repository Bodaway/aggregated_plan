use async_graphql::Object;

use super::enums::QuadrantGql;
use super::task::TaskGql;

/// A task with its computed Eisenhower quadrant for the priority matrix view.
pub struct TaskWithQuadrantGql {
    pub task: TaskGql,
    pub quadrant: QuadrantGql,
}

#[Object]
impl TaskWithQuadrantGql {
    async fn task(&self) -> &TaskGql {
        &self.task
    }

    async fn quadrant(&self) -> QuadrantGql {
        self.quadrant
    }
}

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
