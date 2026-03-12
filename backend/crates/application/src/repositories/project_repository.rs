use async_trait::async_trait;
use domain::types::*;

use crate::errors::RepositoryError;

/// Repository trait for persisting and querying projects.
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    /// Find a project by its unique identifier.
    async fn find_by_id(&self, id: ProjectId) -> Result<Option<Project>, RepositoryError>;

    /// Find all projects for a user.
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<Project>, RepositoryError>;

    /// Find a project by its external source and source-specific identifier.
    async fn find_by_source(
        &self,
        user_id: UserId,
        source: Source,
        source_id: &str,
    ) -> Result<Option<Project>, RepositoryError>;

    /// Save a new project or update an existing one.
    async fn save(&self, project: &Project) -> Result<(), RepositoryError>;

    /// Delete a project by its identifier.
    async fn delete(&self, id: ProjectId) -> Result<(), RepositoryError>;
}
