use crate::types::*;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),
    #[error("Project not found: {0}")]
    ProjectNotFound(ProjectId),
    #[error("Invalid urgency value: {0}. Must be 1-4.")]
    InvalidUrgency(u8),
    #[error("Invalid impact value: {0}. Must be 1-4.")]
    InvalidImpact(u8),
    #[error("Activity slot overlap: existing slot covers this time range")]
    ActivitySlotOverlap,
    #[error("Invalid date range: start {start} is after end {end}")]
    InvalidDateRange { start: String, end: String },
}
