use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::TagRepository;
use domain::types::*;

pub struct SqliteTagRepository {
    pool: SqlitePool,
}

impl SqliteTagRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn map_tag_row(row: &SqliteRow) -> Result<Tag, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");

    Ok(Tag {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        name: Row::get(row, "name"),
        color: Row::get(row, "color"),
    })
}

#[async_trait]
impl TagRepository for SqliteTagRepository {
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<Tag>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM tags WHERE user_id = ? ORDER BY name")
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(map_tag_row).collect()
    }

    async fn find_by_id(&self, id: TagId) -> Result<Option<Tag>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM tags WHERE id = ?")
            .bind(id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(map_tag_row(row)?)),
            None => Ok(None),
        }
    }

    async fn save(&self, tag: &Tag) -> Result<(), RepositoryError> {
        sqlx::query("INSERT INTO tags (id, user_id, name, color) VALUES (?, ?, ?, ?)")
            .bind(tag.id.to_string())
            .bind(tag.user_id.to_string())
            .bind(&tag.name)
            .bind(&tag.color)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn update(&self, tag: &Tag) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE tags SET name = ?, color = ? WHERE id = ?")
            .bind(&tag.name)
            .bind(&tag.color)
            .bind(tag.id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: TagId) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::create_sqlite_pool;

    async fn setup() -> SqlitePool {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT OR IGNORE INTO users (id, name, email, created_at) VALUES (?, ?, ?, ?)")
            .bind("00000000-0000-0000-0000-000000000001")
            .bind("Test User")
            .bind("test@example.com")
            .bind("2024-01-01T00:00:00+00:00")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn make_tag(name: &str, color: Option<&str>) -> Tag {
        Tag {
            id: Uuid::new_v4(),
            user_id: user_id(),
            name: name.to_string(),
            color: color.map(|c| c.to_string()),
        }
    }

    #[tokio::test]
    async fn test_save_and_find_by_id() {
        let pool = setup().await;
        let repo = SqliteTagRepository::new(pool);
        let tag = make_tag("urgent", Some("#ff0000"));

        repo.save(&tag).await.unwrap();
        let found = repo.find_by_id(tag.id).await.unwrap();

        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "urgent");
        assert_eq!(found.color, Some("#ff0000".to_string()));
    }

    #[tokio::test]
    async fn test_find_by_user() {
        let pool = setup().await;
        let repo = SqliteTagRepository::new(pool);

        repo.save(&make_tag("alpha", None)).await.unwrap();
        repo.save(&make_tag("beta", Some("#00ff00"))).await.unwrap();

        let tags = repo.find_by_user(user_id()).await.unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "alpha");
        assert_eq!(tags[1].name, "beta");
    }

    #[tokio::test]
    async fn test_update() {
        let pool = setup().await;
        let repo = SqliteTagRepository::new(pool);
        let mut tag = make_tag("old name", None);

        repo.save(&tag).await.unwrap();

        tag.name = "new name".to_string();
        tag.color = Some("#0000ff".to_string());
        repo.update(&tag).await.unwrap();

        let found = repo.find_by_id(tag.id).await.unwrap().unwrap();
        assert_eq!(found.name, "new name");
        assert_eq!(found.color, Some("#0000ff".to_string()));
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = setup().await;
        let repo = SqliteTagRepository::new(pool);
        let tag = make_tag("to delete", None);

        repo.save(&tag).await.unwrap();
        assert!(repo.find_by_id(tag.id).await.unwrap().is_some());

        repo.delete(tag.id).await.unwrap();
        assert!(repo.find_by_id(tag.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_find_by_id_not_found() {
        let pool = setup().await;
        let repo = SqliteTagRepository::new(pool);
        let found = repo.find_by_id(Uuid::new_v4()).await.unwrap();
        assert!(found.is_none());
    }
}
