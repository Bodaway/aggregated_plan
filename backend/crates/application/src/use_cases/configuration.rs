use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::*;

/// Get a single configuration value for a user.
pub async fn get_config(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    key: &str,
) -> Result<Option<String>, AppError> {
    config_repo.get(user_id, key).await.map_err(Into::into)
}

/// Get all configuration key-value pairs for a user.
pub async fn get_all_config(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
) -> Result<Vec<(String, String)>, AppError> {
    config_repo.get_all(user_id).await.map_err(Into::into)
}

/// Set a configuration value for a user.
pub async fn set_config(
    config_repo: &dyn ConfigRepository,
    user_id: UserId,
    key: &str,
    value: &str,
) -> Result<(), AppError> {
    config_repo.set(user_id, key, value).await.map_err(Into::into)
}

/// Manage tags: create a new tag for a user.
pub async fn create_tag(
    tag_repo: &dyn TagRepository,
    user_id: UserId,
    name: String,
    color: Option<String>,
) -> Result<Tag, AppError> {
    let tag = Tag {
        id: Uuid::new_v4(),
        user_id,
        name,
        color,
    };

    tag_repo.save(&tag).await?;
    Ok(tag)
}

/// Update an existing tag.
pub async fn update_tag(
    tag_repo: &dyn TagRepository,
    tag_id: TagId,
    name: Option<String>,
    color: Option<Option<String>>,
) -> Result<Tag, AppError> {
    let mut tag = tag_repo
        .find_by_id(tag_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tag {}", tag_id)))?;

    if let Some(n) = name {
        tag.name = n;
    }
    if let Some(c) = color {
        tag.color = c;
    }

    tag_repo.update(&tag).await?;
    Ok(tag)
}

/// Delete a tag by its identifier.
pub async fn delete_tag(
    tag_repo: &dyn TagRepository,
    tag_id: TagId,
) -> Result<(), AppError> {
    tag_repo.delete(tag_id).await.map_err(Into::into)
}

/// Get all tags for a user.
pub async fn get_tags(
    tag_repo: &dyn TagRepository,
    user_id: UserId,
) -> Result<Vec<Tag>, AppError> {
    tag_repo.find_by_user(user_id).await.map_err(Into::into)
}
