use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::TimesheetDraftRepository;
use domain::types::*;

pub struct SqliteTimesheetDraftRepository {
    pool: SqlitePool,
}

impl SqliteTimesheetDraftRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Database(format!("bad datetime '{s}': {e}")))
}

fn conf_from(s: &str) -> Confidence {
    match s {
        "high" => Confidence::High,
        "medium" => Confidence::Medium,
        _ => Confidence::Low,
    }
}

fn map_line(row: &SqliteRow) -> Result<TimesheetDraftLine, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let refs_json: Option<String> = Row::get(row, "source_refs_json");
    let is_pinned: i64 = Row::get(row, "is_pinned");
    let conf: String = Row::get(row, "confidence");
    let source_refs: Vec<String> = match refs_json {
        None => vec![],
        Some(j) => serde_json::from_str(&j)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?,
    };
    Ok(TimesheetDraftLine {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        gryzzly_project_id: Row::get(row, "gryzzly_project_id"),
        project_name: Row::get(row, "project_name"),
        hours: Row::get(row, "hours"),
        is_pinned: is_pinned != 0,
        confidence: conf_from(&conf),
        source_refs,
    })
}

fn map_share(row: &SqliteRow) -> Result<QuarterShareRow, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let task_id: Option<String> = Row::get(row, "task_id");
    let quarter_index: i64 = Row::get(row, "quarter_index");
    let is_pinned: i64 = Row::get(row, "is_pinned");
    Ok(QuarterShareRow {
        id: Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        quarter_index: quarter_index as u8,
        task_id: task_id
            .map(|t| Uuid::parse_str(&t))
            .transpose()
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        lane_key: Row::get(row, "lane_key"),
        label: Row::get(row, "label"),
        gryzzly_project_id: Row::get(row, "gryzzly_project_id"),
        presence_minutes: Row::get(row, "presence_minutes"),
        hours: Row::get(row, "hours"),
        is_pinned: is_pinned != 0,
    })
}

#[async_trait]
impl TimesheetDraftRepository for SqliteTimesheetDraftRepository {
    async fn upsert(&self, draft: &TimesheetDraft) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Header upsert (unique on user_id, date).
        sqlx::query(
            "INSERT INTO timesheet_drafts
                (id, user_id, date, status, target_hours, total_hours, day_confidence, blocks_json, unresolved_json, lanes_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, date) DO UPDATE SET
                status = excluded.status, target_hours = excluded.target_hours,
                total_hours = excluded.total_hours, day_confidence = excluded.day_confidence,
                blocks_json = excluded.blocks_json, unresolved_json = excluded.unresolved_json,
                lanes_json = excluded.lanes_json,
                updated_at = excluded.updated_at",
        )
        .bind(draft.id.to_string())
        .bind(draft.user_id.to_string())
        .bind(draft.date.format("%Y-%m-%d").to_string())
        .bind(draft.status.as_str())
        .bind(draft.target_hours)
        .bind(draft.total_hours)
        .bind(draft.day_confidence.as_str())
        .bind(&draft.blocks_json)
        .bind(&draft.unresolved_json)
        .bind(&draft.lanes_json)
        .bind(draft.created_at.to_rfc3339())
        .bind(draft.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Resolve the header id (existing row may keep its original id).
        let header_id: String = sqlx::query("SELECT id FROM timesheet_drafts WHERE user_id = ? AND date = ?")
            .bind(draft.user_id.to_string())
            .bind(draft.date.format("%Y-%m-%d").to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .get::<String, _>("id");

        // Replace lines.
        sqlx::query("DELETE FROM timesheet_draft_lines WHERE draft_id = ?")
            .bind(&header_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        for line in &draft.lines {
            let refs = serde_json::to_string(&line.source_refs)
                .map_err(|e| RepositoryError::Serialization(e.to_string()))?;
            sqlx::query(
                "INSERT INTO timesheet_draft_lines
                    (id, draft_id, gryzzly_project_id, project_name, hours, is_pinned, confidence, source_refs_json, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(line.id.to_string())
            .bind(&header_id)
            .bind(&line.gryzzly_project_id)
            .bind(&line.project_name)
            .bind(line.hours)
            .bind(if line.is_pinned { 1 } else { 0 })
            .bind(line.confidence.as_str())
            .bind(refs)
            .bind(draft.updated_at.to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        }

        // Replace shares. Same contract as lines: the caller owns the whole day's
        // arbitration, pins included, so a partial write would drop declared hours.
        sqlx::query("DELETE FROM timesheet_quarter_shares WHERE draft_id = ?")
            .bind(&header_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        for s in &draft.shares {
            sqlx::query(
                "INSERT INTO timesheet_quarter_shares
                    (id, draft_id, quarter_index, task_id, lane_key, label, gryzzly_project_id,
                     presence_minutes, hours, is_pinned, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(s.id.to_string())
            .bind(&header_id)
            .bind(s.quarter_index as i64)
            .bind(s.task_id.map(|t| t.to_string()))
            .bind(&s.lane_key)
            .bind(&s.label)
            .bind(&s.gryzzly_project_id)
            .bind(s.presence_minutes)
            .bind(s.hours)
            .bind(if s.is_pinned { 1 } else { 0 })
            .bind(draft.updated_at.to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        }

        tx.commit().await.map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn find_by_user_and_date(
        &self,
        user_id: UserId,
        date: NaiveDate,
    ) -> Result<Option<TimesheetDraft>, RepositoryError> {
        let header = sqlx::query("SELECT * FROM timesheet_drafts WHERE user_id = ? AND date = ? LIMIT 1")
            .bind(user_id.to_string())
            .bind(date.format("%Y-%m-%d").to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let Some(h) = header.first() else { return Ok(None) };

        let header_id: String = Row::get(h, "id");
        let line_rows = sqlx::query("SELECT * FROM timesheet_draft_lines WHERE draft_id = ?")
            .bind(&header_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let lines: Result<Vec<_>, _> = line_rows.iter().map(map_line).collect();

        let share_rows = sqlx::query(
            "SELECT * FROM timesheet_quarter_shares WHERE draft_id = ?
             ORDER BY quarter_index, hours DESC, label",
        )
        .bind(&header_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let shares: Result<Vec<_>, _> = share_rows.iter().map(map_share).collect();

        let status_str: String = Row::get(h, "status");
        let conf_str: String = Row::get(h, "day_confidence");
        Ok(Some(TimesheetDraft {
            id: Uuid::parse_str(&header_id).map_err(|e| RepositoryError::Database(e.to_string()))?,
            user_id,
            date,
            status: TimesheetStatus::from_str(&status_str)
                .ok_or_else(|| RepositoryError::Database(format!("bad status '{status_str}'")))?,
            target_hours: Row::get(h, "target_hours"),
            total_hours: Row::get(h, "total_hours"),
            day_confidence: conf_from(&conf_str),
            blocks_json: Row::get(h, "blocks_json"),
            unresolved_json: Row::get(h, "unresolved_json"),
            lanes_json: Row::get(h, "lanes_json"),
            lines: lines?,
            shares: shares?,
            created_at: parse_dt(&Row::get::<String, _>(h, "created_at"))?,
            updated_at: parse_dt(&Row::get::<String, _>(h, "updated_at"))?,
        }))
    }

    async fn set_status(
        &self,
        user_id: UserId,
        date: NaiveDate,
        status: TimesheetStatus,
    ) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE timesheet_drafts SET status = ? WHERE user_id = ? AND date = ?")
            .bind(status.as_str())
            .bind(user_id.to_string())
            .bind(date.format("%Y-%m-%d").to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    async fn pool_with_user() -> (SqlitePool, Uuid) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../../migrations/sqlite").run(&pool).await.unwrap();
        let uid = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, name, email, created_at) VALUES (?, 'T', 't@e.co', ?)")
            .bind(uid.to_string())
            .bind(Utc::now().to_rfc3339())
            .execute(&pool)
            .await
            .unwrap();
        (pool, uid)
    }

    fn draft(uid: Uuid) -> TimesheetDraft {
        TimesheetDraft {
            id: Uuid::new_v4(),
            user_id: uid,
            date: NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(),
            status: TimesheetStatus::Draft,
            target_hours: 7.5,
            total_hours: 7.5,
            day_confidence: Confidence::High,
            blocks_json: Some("[]".into()),
            unresolved_json: Some(
                r#"[{"sourceRef":"wl:1","label":"note sans projet","at":"2026-06-08 09:00:00"}]"#
                    .into(),
            ),
            lanes_json: None,
            lines: vec![TimesheetDraftLine {
                id: Uuid::new_v4(),
                gryzzly_project_id: Some("p1".into()),
                project_name: Some("Proj 1".into()),
                hours: 7.5,
                is_pinned: false,
                confidence: Confidence::High,
                source_refs: vec!["wl-1".into()],
            }],
            shares: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn upsert_then_find_roundtrips() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let d = draft(uid);
        repo.upsert(&d).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.lines.len(), 1);
        assert_eq!(got.lines[0].gryzzly_project_id.as_deref(), Some("p1"));
        assert_eq!(got.status, TimesheetStatus::Draft);
    }

    #[tokio::test]
    async fn upsert_replaces_lines_not_appends() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let mut d = draft(uid);
        repo.upsert(&d).await.unwrap();
        d.lines[0].hours = 3.0;
        repo.upsert(&d).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.lines.len(), 1, "re-upsert must replace lines");
        assert!((got.lines[0].hours - 3.0).abs() < 1e-9);
    }

    /// The unresolved-signal list is the only record of WHAT went unattributed; it must
    /// survive the round trip (and a re-upsert), or every page load loses the explanation.
    #[tokio::test]
    async fn upsert_then_find_roundtrips_unresolved_json() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let mut d = draft(uid);
        repo.upsert(&d).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.unresolved_json, d.unresolved_json);

        d.unresolved_json = Some(r#"[]"#.into());
        repo.upsert(&d).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.unresolved_json.as_deref(), Some("[]"), "re-upsert must update the column");
    }

    fn share(quarter_index: u8, lane_key: &str, hours: f64, is_pinned: bool) -> QuarterShareRow {
        QuarterShareRow {
            id: Uuid::new_v4(),
            quarter_index,
            task_id: None,
            lane_key: lane_key.into(),
            label: lane_key.into(),
            gryzzly_project_id: Some("p1".into()),
            presence_minutes: 98,
            hours,
            is_pinned,
        }
    }

    /// Shares are billing decisions: every field has to survive the round trip, the
    /// pinned flag most of all — it is what a re-reconstruct reads to know which hours
    /// the user set by hand and must not recompute.
    #[tokio::test]
    async fn upsert_then_find_roundtrips_quarter_shares() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let mut d = draft(uid);
        d.shares = vec![share(3, "task:a", 0.75, true), share(3, "task:b", 1.25, false)];
        repo.upsert(&d).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.shares.len(), 2);
        let a = got.shares.iter().find(|s| s.lane_key == "task:a").unwrap();
        assert!(a.is_pinned, "the pin is the user's decision, it must persist");
        assert_eq!(a.presence_minutes, 98);
        assert!((a.hours - 0.75).abs() < 1e-9);
        assert_eq!(a.quarter_index, 3);
    }

    #[tokio::test]
    async fn upsert_replaces_shares_not_appends() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let mut d = draft(uid);
        d.shares = vec![share(0, "task:a", 2.0, false)];
        repo.upsert(&d).await.unwrap();
        d.shares = vec![share(0, "task:a", 1.0, false), share(0, "task:b", 1.0, false)];
        repo.upsert(&d).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.shares.len(), 2, "a re-upsert replaces the quarter, never doubles it");
        assert!((got.shares.iter().map(|s| s.hours).sum::<f64>() - 2.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn upsert_then_find_roundtrips_lanes_json() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let mut d = draft(uid);
        d.lanes_json = Some(r#"[{"laneKey":"task:a","intervals":[[540,600]]}]"#.into());
        repo.upsert(&d).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.lanes_json, d.lanes_json);
    }

    #[tokio::test]
    async fn set_status_updates_only_status() {
        let (pool, uid) = pool_with_user().await;
        let repo = SqliteTimesheetDraftRepository::new(pool);
        let d = draft(uid);
        repo.upsert(&d).await.unwrap();
        repo.set_status(uid, d.date, TimesheetStatus::Validated).await.unwrap();
        let got = repo.find_by_user_and_date(uid, d.date).await.unwrap().unwrap();
        assert_eq!(got.status, TimesheetStatus::Validated);
    }
}
