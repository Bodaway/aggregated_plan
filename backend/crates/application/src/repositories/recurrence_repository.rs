use async_trait::async_trait;
use domain::types::common::UserId;
use domain::types::recurrence::{RecurrenceTemplate, RecurrenceTemplateId};

use crate::errors::RepositoryError;

/// Repository trait for persisting and querying recurrence templates.
#[async_trait]
pub trait RecurrenceRepository: Send + Sync {
    /// Find a template by its unique identifier.
    async fn find_by_id(
        &self,
        id: RecurrenceTemplateId,
    ) -> Result<Option<RecurrenceTemplate>, RepositoryError>;

    /// Find all active templates for a user.
    async fn find_active_by_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<RecurrenceTemplate>, RepositoryError>;

    /// Save a new template or update an existing one.
    async fn save(&self, template: &RecurrenceTemplate) -> Result<(), RepositoryError>;

    /// Soft-delete a template (sets `active = false`).
    async fn deactivate(&self, id: RecurrenceTemplateId) -> Result<(), RepositoryError>;
}
