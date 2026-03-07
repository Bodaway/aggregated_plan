use async_trait::async_trait;
use chrono::NaiveDate;
use domain::types::*;

use crate::errors::RepositoryError;

/// Repository trait for persisting and querying activity slots.
#[async_trait]
pub trait ActivitySlotRepository: Send + Sync {
    /// Find an activity slot by its unique identifier.
    async fn find_by_id(&self, id: ActivitySlotId) -> Result<Option<ActivitySlot>, RepositoryError>;

    /// Find all activity slots for a user on a specific date.
    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<Vec<ActivitySlot>, RepositoryError>;

    /// Find the currently active (no end_time) slot for a user.
    async fn find_active(
        &self,
        user_id: UserId,
    ) -> Result<Option<ActivitySlot>, RepositoryError>;

    /// Save a new activity slot.
    async fn save(&self, slot: &ActivitySlot) -> Result<(), RepositoryError>;

    /// Update an existing activity slot.
    async fn update(&self, slot: &ActivitySlot) -> Result<(), RepositoryError>;

    /// Delete an activity slot by its identifier.
    async fn delete(&self, id: ActivitySlotId) -> Result<(), RepositoryError>;
}
