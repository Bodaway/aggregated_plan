use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// Get a single configuration value for a user.
pub async fn get_config(
    _config_repo: &dyn ConfigRepository,
    _user_id: UserId,
    _key: &str,
) -> Result<Option<String>, AppError> {
    todo!()
}

/// Get all configuration key-value pairs for a user.
pub async fn get_all_config(
    _config_repo: &dyn ConfigRepository,
    _user_id: UserId,
) -> Result<Vec<(String, String)>, AppError> {
    todo!()
}

/// Set a configuration value for a user.
pub async fn set_config(
    _config_repo: &dyn ConfigRepository,
    _user_id: UserId,
    _key: &str,
    _value: &str,
) -> Result<(), AppError> {
    todo!()
}

/// Manage tags: create a new tag for a user.
pub async fn create_tag(
    _tag_repo: &dyn TagRepository,
    _user_id: UserId,
    _name: String,
    _color: Option<String>,
) -> Result<Tag, AppError> {
    todo!()
}

/// Update an existing tag.
pub async fn update_tag(
    _tag_repo: &dyn TagRepository,
    _tag_id: TagId,
    _name: Option<String>,
    _color: Option<Option<String>>,
) -> Result<Tag, AppError> {
    todo!()
}

/// Delete a tag by its identifier.
pub async fn delete_tag(
    _tag_repo: &dyn TagRepository,
    _tag_id: TagId,
) -> Result<(), AppError> {
    todo!()
}

/// Get all tags for a user.
pub async fn get_tags(
    _tag_repo: &dyn TagRepository,
    _user_id: UserId,
) -> Result<Vec<Tag>, AppError> {
    todo!()
}
