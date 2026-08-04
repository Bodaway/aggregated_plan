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

    /// Find all completed activity slots for a user within a date range (inclusive).
    async fn find_by_user_and_date_range(
        &self,
        user_id: UserId,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<ActivitySlot>, RepositoryError>;

    /// Delete an activity slot by its identifier.
    async fn delete(&self, id: ActivitySlotId) -> Result<(), RepositoryError>;

    /// Stamp `source` on the given slots. Returns how many rows moved.
    ///
    /// Loud default, like the rest of the added trait methods in this crate: a double
    /// that silently reported `0` would make the classification pass look finished
    /// while every row stayed NULL — and a NULL row is one a rebuild will not touch,
    /// so the failure would surface weeks later as a double-counted morning.
    async fn set_source(
        &self,
        _ids: &[ActivitySlotId],
        _source: SlotSource,
    ) -> Result<u64, RepositoryError> {
        Err(RepositoryError::Database(
            "set_source is not implemented by this repository".into(),
        ))
    }
}
