use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::SessionRepository;
use domain::types::*;

use super::conversions::{session_mode_from_str, session_mode_to_str};

pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Serialization(format!("bad timestamp `{s}`: {e}")))
}

fn row_to_session(row: &sqlx::sqlite::SqliteRow) -> Result<Session, RepositoryError> {
    let task_id: Option<String> = Row::get(row, "task_id");
    let task_id = match task_id {
        Some(raw) => Some(
            Uuid::parse_str(&raw)
                .map_err(|e| RepositoryError::Serialization(format!("bad task id: {e}")))?,
        ),
        None => None,
    };
    let user_id: String = Row::get(row, "user_id");
    let mode: String = Row::get(row, "mode");
    let started_at: String = Row::get(row, "started_at");
    let last_seen_at: String = Row::get(row, "last_seen_at");
    let last_flush_at: Option<String> = Row::get(row, "last_flush_at");
    let ended_at: Option<String> = Row::get(row, "ended_at");

    Ok(Session {
        id: Row::get(row, "id"),
        user_id: Uuid::parse_str(&user_id)
            .map_err(|e| RepositoryError::Serialization(format!("bad user id: {e}")))?,
        task_id,
        mode: session_mode_from_str(&mode),
        label: Row::get(row, "label"),
        started_at: parse_datetime(&started_at)?,
        last_seen_at: parse_datetime(&last_seen_at)?,
        last_flush_at: last_flush_at.as_deref().map(parse_datetime).transpose()?,
        ended_at: ended_at.as_deref().map(parse_datetime).transpose()?,
    })
}

#[async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn find_by_id(
        &self,
        id: &str,
        user_id: UserId,
    ) -> Result<Option<Session>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, user_id, task_id, mode, label, started_at, last_seen_at,
                    last_flush_at, ended_at
             FROM sessions WHERE id = ? AND user_id = ?",
        )
        .bind(id)
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => Ok(Some(row_to_session(row)?)),
            None => Ok(None),
        }
    }

    async fn upsert(&self, session: &Session) -> Result<(), RepositoryError> {
        // `started_at` is absent from the UPDATE clause on purpose: a rebind is the
        // same session, and plan 2's flush window is anchored on it. Letting a
        // caller rewrite it would move a window that has already been used.
        //
        // `ended_at` IS in the UPDATE clause: a bind is a request to work, and the
        // use case clears it on the row it passes in when reviving a closed
        // session. Without this, that clear would never persist and `end`'s own
        // `WHERE ended_at IS NULL` guard would make the id unrecoverable.
        sqlx::query(
            "INSERT INTO sessions
                (id, user_id, task_id, mode, label, started_at, last_seen_at, last_flush_at, ended_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                task_id      = excluded.task_id,
                mode         = excluded.mode,
                label        = excluded.label,
                last_seen_at = excluded.last_seen_at,
                ended_at     = excluded.ended_at",
        )
        .bind(&session.id)
        .bind(session.user_id.to_string())
        .bind(session.task_id.map(|t| t.to_string()))
        .bind(session_mode_to_str(session.mode))
        .bind(session.label.as_deref())
        .bind(session.started_at.to_rfc3339())
        .bind(session.last_seen_at.to_rfc3339())
        .bind(session.last_flush_at.map(|d| d.to_rfc3339()))
        .bind(session.ended_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn list_open(&self, user_id: UserId) -> Result<Vec<Session>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, user_id, task_id, mode, label, started_at, last_seen_at,
                    last_flush_at, ended_at
             FROM sessions
             WHERE user_id = ? AND ended_at IS NULL
             ORDER BY last_seen_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(row_to_session).collect()
    }

    async fn list_idle_open(
        &self,
        user_id: UserId,
        idle_before: DateTime<Utc>,
    ) -> Result<Vec<Session>, RepositoryError> {
        // Comparison on RFC 3339 text, same as `list_open`'s ordering: harmless
        // because real clocks carry fractional seconds uniformly, but here it is a
        // `<` filter rather than an ordering.
        let rows = sqlx::query(
            "SELECT id, user_id, task_id, mode, label, started_at, last_seen_at,
                    last_flush_at, ended_at
             FROM sessions
             WHERE user_id = ? AND ended_at IS NULL AND last_seen_at < ?
             ORDER BY last_seen_at ASC",
        )
        .bind(user_id.to_string())
        .bind(idle_before.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        rows.iter().map(row_to_session).collect()
    }

    async fn touch(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query(
            "UPDATE sessions SET last_seen_at = ?
             WHERE id = ? AND user_id = ? AND ended_at IS NULL",
        )
        .bind(at.to_rfc3339())
        .bind(id)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn set_last_flush(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query(
            "UPDATE sessions SET last_flush_at = ? WHERE id = ? AND user_id = ?",
        )
        .bind(at.to_rfc3339())
        .bind(id)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn end(
        &self,
        id: &str,
        user_id: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        // `ended_at IS NULL` in the WHERE is what makes this idempotent: the first
        // close wins, because that is when the work actually stopped.
        let result = sqlx::query(
            "UPDATE sessions SET ended_at = ?
             WHERE id = ? AND user_id = ? AND ended_at IS NULL",
        )
        .bind(at.to_rfc3339())
        .bind(id)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::connection::create_sqlite_pool;
    use chrono::TimeZone;

    async fn setup() -> SqlitePool {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query(
            "INSERT INTO tasks (id, user_id, title, source, status, urgency, impact, created_at, updated_at)
             VALUES (?, ?, 'Tâche', 'personal', 'todo', 2, 2, ?, ?)",
        )
        .bind(task_id().to_string())
        .bind(user_id().to_string())
        .bind(t(8).to_rfc3339())
        .bind(t(8).to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }
    fn task_id() -> TaskId {
        Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap()
    }
    fn t(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 4, h, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn a_session_round_trips_every_column() {
        let repo = SqliteSessionRepository::new(setup().await);
        let session =
            Session::tracking("s1".into(), user_id(), task_id(), Some("/tmp/x".into()), t(9))
                .unwrap();

        repo.upsert(&session).await.unwrap();
        let found = repo.find_by_id("s1", user_id()).await.unwrap().unwrap();

        assert_eq!(found.id, "s1");
        assert_eq!(found.task_id, Some(task_id()));
        assert_eq!(found.mode, SessionMode::Tracking);
        assert_eq!(found.label.as_deref(), Some("/tmp/x"));
        assert_eq!(found.started_at, t(9));
        assert_eq!(found.last_seen_at, t(9));
        assert!(found.last_flush_at.is_none());
        assert!(found.ended_at.is_none());
    }

    #[tokio::test]
    async fn upsert_keeps_started_at_and_overwrites_the_rest() {
        let repo = SqliteSessionRepository::new(setup().await);
        let mut session =
            Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap();
        repo.upsert(&session).await.unwrap();

        session.started_at = t(15); // a caller that got this wrong must not win
        session.last_seen_at = t(15);
        session.mode = SessionMode::Off;
        session.task_id = None;
        repo.upsert(&session).await.unwrap();

        let found = repo.find_by_id("s1", user_id()).await.unwrap().unwrap();
        assert_eq!(found.started_at, t(9), "started_at is written once");
        assert_eq!(found.last_seen_at, t(15));
        assert_eq!(found.mode, SessionMode::Off);
        assert!(found.task_id.is_none());
    }

    #[tokio::test]
    async fn a_bind_after_end_reopens_the_session() {
        // The end-to-end regression for the defect: `session bind` after
        // `session end` used to report success and change nothing, because
        // `ended_at` was absent from the UPDATE clause. This is the sequence
        // that poisoned an id for good — `end`'s own `WHERE ended_at IS NULL`
        // guard means no shipped verb could otherwise reopen the row.
        let repo = SqliteSessionRepository::new(setup().await);
        let mut session =
            Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap();
        repo.upsert(&session).await.unwrap();
        repo.end("s1", user_id(), t(12)).await.unwrap();

        session.ended_at = None;
        session.last_seen_at = t(14);
        repo.upsert(&session).await.unwrap();

        let found = repo.find_by_id("s1", user_id()).await.unwrap().unwrap();
        assert!(found.ended_at.is_none(), "the bind must reopen the row");
        assert_eq!(found.task_id, Some(task_id()));

        let open = repo.list_open(user_id()).await.unwrap();
        let ids: Vec<&str> = open.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["s1"], "list_open must show the revived session again");
    }

    #[tokio::test]
    async fn find_by_id_is_scoped_to_the_user() {
        let repo = SqliteSessionRepository::new(setup().await);
        let session =
            Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap();
        repo.upsert(&session).await.unwrap();

        let other = Uuid::parse_str("00000000-0000-0000-0000-0000000000ff").unwrap();
        assert!(repo.find_by_id("s1", other).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn touch_and_set_last_flush_move_only_their_own_column() {
        let repo = SqliteSessionRepository::new(setup().await);
        repo.upsert(&Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap())
            .await
            .unwrap();

        assert!(repo.touch("s1", user_id(), t(11)).await.unwrap());
        assert!(repo.set_last_flush("s1", user_id(), t(12)).await.unwrap());

        let found = repo.find_by_id("s1", user_id()).await.unwrap().unwrap();
        assert_eq!(found.last_seen_at, t(11));
        assert_eq!(found.last_flush_at, Some(t(12)));
        assert_eq!(found.started_at, t(9));
    }

    #[tokio::test]
    async fn touch_reports_false_for_an_unknown_or_ended_session() {
        let repo = SqliteSessionRepository::new(setup().await);
        assert!(!repo.touch("ghost", user_id(), t(11)).await.unwrap());

        repo.upsert(&Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap())
            .await
            .unwrap();
        repo.end("s1", user_id(), t(17)).await.unwrap();
        assert!(
            !repo.touch("s1", user_id(), t(18)).await.unwrap(),
            "an ended session is not alive"
        );
    }

    #[tokio::test]
    async fn end_is_idempotent() {
        let repo = SqliteSessionRepository::new(setup().await);
        repo.upsert(&Session::tracking("s1".into(), user_id(), task_id(), None, t(9)).unwrap())
            .await
            .unwrap();

        assert!(repo.end("s1", user_id(), t(17)).await.unwrap());
        assert!(!repo.end("s1", user_id(), t(19)).await.unwrap());
        let found = repo.find_by_id("s1", user_id()).await.unwrap().unwrap();
        assert_eq!(found.ended_at, Some(t(17)));
    }

    #[tokio::test]
    async fn list_open_excludes_ended_and_orders_by_last_seen_desc() {
        let repo = SqliteSessionRepository::new(setup().await);
        for (id, hour) in [("s1", 9), ("s2", 11), ("s3", 10)] {
            repo.upsert(
                &Session::tracking(id.into(), user_id(), task_id(), None, t(hour)).unwrap(),
            )
            .await
            .unwrap();
        }
        repo.end("s3", user_id(), t(12)).await.unwrap();

        let open = repo.list_open(user_id()).await.unwrap();

        let ids: Vec<&str> = open.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["s2", "s1"]);
    }

    #[tokio::test]
    async fn list_idle_open_returns_only_stale_open_sessions() {
        let repo = SqliteSessionRepository::new(setup().await);
        // Seen recently — alive.
        repo.upsert(&Session::tracking("fresh".into(), user_id(), task_id(), None, t(16)).unwrap())
            .await
            .unwrap();
        // Seen long ago — idle.
        repo.upsert(&Session::tracking("stale".into(), user_id(), task_id(), None, t(2)).unwrap())
            .await
            .unwrap();
        // Idle but already closed — not ours to reap twice.
        repo.upsert(&Session::tracking("closed".into(), user_id(), task_id(), None, t(2)).unwrap())
            .await
            .unwrap();
        repo.end("closed", user_id(), t(3)).await.unwrap();

        let idle = repo.list_idle_open(user_id(), t(10)).await.unwrap();

        let ids: Vec<&str> = idle.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["stale"]);
    }

    #[tokio::test]
    async fn list_idle_open_is_scoped_to_the_user() {
        let repo = SqliteSessionRepository::new(setup().await);
        repo.upsert(&Session::tracking("stale".into(), user_id(), task_id(), None, t(2)).unwrap())
            .await
            .unwrap();

        let other = Uuid::parse_str("00000000-0000-0000-0000-0000000000ff").unwrap();
        assert!(repo.list_idle_open(other, t(10)).await.unwrap().is_empty());
    }
}
