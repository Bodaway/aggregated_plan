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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::errors::RepositoryError;

    // ---- In-memory repos ----

    struct InMemoryTagRepository {
        tags: Mutex<HashMap<TagId, Tag>>,
    }

    impl InMemoryTagRepository {
        fn new() -> Self {
            Self {
                tags: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl TagRepository for InMemoryTagRepository {
        async fn find_by_user(&self, user_id: UserId) -> Result<Vec<Tag>, RepositoryError> {
            let tags = self.tags.lock().unwrap();
            Ok(tags
                .values()
                .filter(|t| t.user_id == user_id)
                .cloned()
                .collect())
        }

        async fn find_by_id(&self, id: TagId) -> Result<Option<Tag>, RepositoryError> {
            let tags = self.tags.lock().unwrap();
            Ok(tags.get(&id).cloned())
        }

        async fn save(&self, tag: &Tag) -> Result<(), RepositoryError> {
            let mut tags = self.tags.lock().unwrap();
            tags.insert(tag.id, tag.clone());
            Ok(())
        }

        async fn update(&self, tag: &Tag) -> Result<(), RepositoryError> {
            let mut tags = self.tags.lock().unwrap();
            tags.insert(tag.id, tag.clone());
            Ok(())
        }

        async fn delete(&self, id: TagId) -> Result<(), RepositoryError> {
            let mut tags = self.tags.lock().unwrap();
            tags.remove(&id);
            Ok(())
        }
    }

    struct InMemoryConfigRepository {
        store: Mutex<HashMap<(UserId, String), String>>,
    }

    impl InMemoryConfigRepository {
        fn new() -> Self {
            Self {
                store: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl ConfigRepository for InMemoryConfigRepository {
        async fn get(
            &self,
            user_id: UserId,
            key: &str,
        ) -> Result<Option<String>, RepositoryError> {
            let store = self.store.lock().unwrap();
            Ok(store.get(&(user_id, key.to_string())).cloned())
        }

        async fn get_all(
            &self,
            user_id: UserId,
        ) -> Result<Vec<(String, String)>, RepositoryError> {
            let store = self.store.lock().unwrap();
            Ok(store
                .iter()
                .filter(|((uid, _), _)| *uid == user_id)
                .map(|((_, k), v)| (k.clone(), v.clone()))
                .collect())
        }

        async fn set(
            &self,
            user_id: UserId,
            key: &str,
            value: &str,
        ) -> Result<(), RepositoryError> {
            let mut store = self.store.lock().unwrap();
            store.insert((user_id, key.to_string()), value.to_string());
            Ok(())
        }
    }

    fn test_user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    // ---- Tag tests ----

    #[tokio::test]
    async fn create_tag_returns_tag_with_name() {
        let repo = InMemoryTagRepository::new();
        let tag = create_tag(&repo, test_user_id(), "frontend".to_string(), Some("#ff0000".to_string()))
            .await
            .unwrap();

        assert_eq!(tag.name, "frontend");
        assert_eq!(tag.color, Some("#ff0000".to_string()));
        assert_eq!(tag.user_id, test_user_id());
    }

    #[tokio::test]
    async fn create_tag_without_color() {
        let repo = InMemoryTagRepository::new();
        let tag = create_tag(&repo, test_user_id(), "backend".to_string(), None)
            .await
            .unwrap();

        assert_eq!(tag.name, "backend");
        assert!(tag.color.is_none());
    }

    #[tokio::test]
    async fn update_tag_changes_name() {
        let repo = InMemoryTagRepository::new();
        let tag = create_tag(&repo, test_user_id(), "old".to_string(), None)
            .await
            .unwrap();

        let updated = update_tag(&repo, tag.id, Some("new".to_string()), None)
            .await
            .unwrap();

        assert_eq!(updated.name, "new");
        assert!(updated.color.is_none());
    }

    #[tokio::test]
    async fn update_tag_changes_color() {
        let repo = InMemoryTagRepository::new();
        let tag = create_tag(&repo, test_user_id(), "tag".to_string(), None)
            .await
            .unwrap();

        let updated = update_tag(&repo, tag.id, None, Some(Some("#00ff00".to_string())))
            .await
            .unwrap();

        assert_eq!(updated.name, "tag");
        assert_eq!(updated.color, Some("#00ff00".to_string()));
    }

    #[tokio::test]
    async fn update_tag_clears_color() {
        let repo = InMemoryTagRepository::new();
        let tag = create_tag(&repo, test_user_id(), "tag".to_string(), Some("#ff0000".to_string()))
            .await
            .unwrap();

        let updated = update_tag(&repo, tag.id, None, Some(None))
            .await
            .unwrap();

        assert!(updated.color.is_none());
    }

    #[tokio::test]
    async fn update_tag_not_found() {
        let repo = InMemoryTagRepository::new();
        let result = update_tag(&repo, Uuid::new_v4(), Some("x".to_string()), None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_tag_removes_it() {
        let repo = InMemoryTagRepository::new();
        let tag = create_tag(&repo, test_user_id(), "delete-me".to_string(), None)
            .await
            .unwrap();

        delete_tag(&repo, tag.id).await.unwrap();

        let tags = get_tags(&repo, test_user_id()).await.unwrap();
        assert!(tags.is_empty());
    }

    #[tokio::test]
    async fn get_tags_returns_all_for_user() {
        let repo = InMemoryTagRepository::new();

        create_tag(&repo, test_user_id(), "a".to_string(), None)
            .await
            .unwrap();
        create_tag(&repo, test_user_id(), "b".to_string(), None)
            .await
            .unwrap();
        create_tag(&repo, test_user_id(), "c".to_string(), None)
            .await
            .unwrap();

        let tags = get_tags(&repo, test_user_id()).await.unwrap();
        assert_eq!(tags.len(), 3);
    }

    #[tokio::test]
    async fn get_tags_isolates_by_user() {
        let repo = InMemoryTagRepository::new();
        let other_user = Uuid::new_v4();

        create_tag(&repo, test_user_id(), "mine".to_string(), None)
            .await
            .unwrap();
        create_tag(&repo, other_user, "theirs".to_string(), None)
            .await
            .unwrap();

        let my_tags = get_tags(&repo, test_user_id()).await.unwrap();
        assert_eq!(my_tags.len(), 1);
        assert_eq!(my_tags[0].name, "mine");
    }

    // ---- Config tests ----

    #[tokio::test]
    async fn set_and_get_config() {
        let repo = InMemoryConfigRepository::new();
        let user = test_user_id();

        set_config(&repo, user, "jira.url", "https://jira.example.com")
            .await
            .unwrap();

        let value = get_config(&repo, user, "jira.url").await.unwrap();
        assert_eq!(value, Some("https://jira.example.com".to_string()));
    }

    #[tokio::test]
    async fn get_config_returns_none_for_missing_key() {
        let repo = InMemoryConfigRepository::new();
        let value = get_config(&repo, test_user_id(), "nonexistent")
            .await
            .unwrap();
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn set_config_overwrites_existing() {
        let repo = InMemoryConfigRepository::new();
        let user = test_user_id();

        set_config(&repo, user, "key", "v1").await.unwrap();
        set_config(&repo, user, "key", "v2").await.unwrap();

        let value = get_config(&repo, user, "key").await.unwrap();
        assert_eq!(value, Some("v2".to_string()));
    }

    #[tokio::test]
    async fn get_all_config_returns_pairs() {
        let repo = InMemoryConfigRepository::new();
        let user = test_user_id();

        set_config(&repo, user, "a", "1").await.unwrap();
        set_config(&repo, user, "b", "2").await.unwrap();

        let all = get_all_config(&repo, user).await.unwrap();
        assert_eq!(all.len(), 2);

        let keys: Vec<&str> = all.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"b"));
    }

    #[tokio::test]
    async fn get_all_config_isolates_by_user() {
        let repo = InMemoryConfigRepository::new();
        let user1 = test_user_id();
        let user2 = Uuid::new_v4();

        set_config(&repo, user1, "x", "1").await.unwrap();
        set_config(&repo, user2, "y", "2").await.unwrap();

        let user1_config = get_all_config(&repo, user1).await.unwrap();
        assert_eq!(user1_config.len(), 1);
        assert_eq!(user1_config[0].0, "x");
    }
}
