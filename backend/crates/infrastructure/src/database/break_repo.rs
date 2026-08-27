use async_trait::async_trait;
use chrono::{DateTime, NaiveTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::{BreakEventRepository, BreakRuleRepository};
use domain::types::*;

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RepositoryError::Database(format!("bad datetime '{s}': {e}")))
}

fn parse_opt_dt(s: Option<String>) -> Result<Option<DateTime<Utc>>, RepositoryError> {
    s.as_deref().map(parse_dt).transpose()
}

fn parse_uuid(s: &str) -> Result<Uuid, RepositoryError> {
    Uuid::parse_str(s).map_err(|e| RepositoryError::Database(e.to_string()))
}

/// Rebuild the cadence sum type from the two nullable columns the CHECK keeps exclusive.
fn map_cadence(row: &SqliteRow) -> Result<BreakCadence, RepositoryError> {
    let cadence: String = Row::get(row, "cadence");
    match cadence.as_str() {
        "interval" => {
            let minutes: i64 = Row::get(row, "interval_minutes");
            Ok(BreakCadence::Interval { minutes: minutes as u32 })
        }
        "daily" => {
            let at: String = Row::get(row, "at_time");
            let at = NaiveTime::parse_from_str(&at, "%H:%M")
                .map_err(|e| RepositoryError::Database(format!("bad at_time '{at}': {e}")))?;
            Ok(BreakCadence::Daily { at })
        }
        other => Err(RepositoryError::Database(format!("bad cadence '{other}'"))),
    }
}

fn map_rule(row: &SqliteRow) -> Result<BreakRule, RepositoryError> {
    let kind_str: String = Row::get(row, "kind");
    let urgency_str: String = Row::get(row, "urgency");
    let duration: i64 = Row::get(row, "duration_seconds");
    let enabled: i64 = Row::get(row, "enabled");
    Ok(BreakRule {
        id: parse_uuid(&Row::get::<String, _>(row, "id"))?,
        user_id: parse_uuid(&Row::get::<String, _>(row, "user_id"))?,
        kind: BreakKind::from_str(&kind_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad kind '{kind_str}'")))?,
        label: Row::get(row, "label"),
        body: Row::get(row, "body"),
        cadence: map_cadence(row)?,
        duration_seconds: duration as u32,
        priority: Row::get::<i64, _>(row, "priority") as i32,
        enabled: enabled != 0,
        urgency: BreakUrgency::from_str(&urgency_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad urgency '{urgency_str}'")))?,
        created_at: parse_dt(&Row::get::<String, _>(row, "created_at"))?,
        updated_at: parse_dt(&Row::get::<String, _>(row, "updated_at"))?,
    })
}

fn map_event(row: &SqliteRow) -> Result<BreakEvent, RepositoryError> {
    let outcome_str: String = Row::get(row, "outcome");
    let reason: Option<String> = Row::get(row, "defer_reason");
    Ok(BreakEvent {
        id: parse_uuid(&Row::get::<String, _>(row, "id"))?,
        user_id: parse_uuid(&Row::get::<String, _>(row, "user_id"))?,
        rule_id: parse_uuid(&Row::get::<String, _>(row, "rule_id"))?,
        due_at: parse_dt(&Row::get::<String, _>(row, "due_at"))?,
        fired_at: parse_opt_dt(Row::get(row, "fired_at"))?,
        deferred_until: parse_opt_dt(Row::get(row, "deferred_until"))?,
        defer_reason: reason.as_deref().and_then(DeferReason::from_str),
        suppressed_by_meeting_id: Row::get(row, "suppressed_by_meeting_id"),
        outcome: BreakOutcome::from_str(&outcome_str)
            .ok_or_else(|| RepositoryError::Database(format!("bad outcome '{outcome_str}'")))?,
        responded_at: parse_opt_dt(Row::get(row, "responded_at"))?,
        created_at: parse_dt(&Row::get::<String, _>(row, "created_at"))?,
    })
}

pub struct SqliteBreakRuleRepository {
    pool: SqlitePool,
}

impl SqliteBreakRuleRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

const RULE_COLUMNS: &str = "id, user_id, kind, label, body, cadence, interval_minutes, at_time, \
                            duration_seconds, priority, enabled, urgency, created_at, updated_at";

#[async_trait]
impl BreakRuleRepository for SqliteBreakRuleRepository {
    async fn list(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM break_rules WHERE user_id = ? ORDER BY priority ASC, created_at ASC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_rule).collect()
    }

    async fn list_enabled(&self, user_id: UserId) -> Result<Vec<BreakRule>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM break_rules WHERE user_id = ? AND enabled = 1 \
             ORDER BY priority ASC, created_at ASC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_rule).collect()
    }

    async fn get(
        &self,
        user_id: UserId,
        id: BreakRuleId,
    ) -> Result<Option<BreakRule>, RepositoryError> {
        let row = sqlx::query("SELECT * FROM break_rules WHERE user_id = ? AND id = ?")
            .bind(user_id.to_string())
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        row.as_ref().map(map_rule).transpose()
    }

    async fn create(&self, rule: &BreakRule) -> Result<(), RepositoryError> {
        sqlx::query(&format!(
            "INSERT INTO break_rules ({RULE_COLUMNS}) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        ))
        .bind(rule.id.to_string())
        .bind(rule.user_id.to_string())
        .bind(rule.kind.as_str())
        .bind(&rule.label)
        .bind(&rule.body)
        .bind(match rule.cadence {
            BreakCadence::Interval { .. } => "interval",
            BreakCadence::Daily { .. } => "daily",
        })
        .bind(rule.cadence.interval_minutes().map(|m| m as i64))
        .bind(rule.cadence.at_time().map(|t| t.format("%H:%M").to_string()))
        .bind(rule.duration_seconds as i64)
        .bind(rule.priority as i64)
        .bind(i64::from(rule.enabled))
        .bind(rule.urgency.as_str())
        .bind(rule.created_at.to_rfc3339())
        .bind(rule.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, rule: &BreakRule) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE break_rules SET kind = ?, label = ?, body = ?, cadence = ?, \
             interval_minutes = ?, at_time = ?, duration_seconds = ?, priority = ?, \
             enabled = ?, urgency = ?, updated_at = ? WHERE user_id = ? AND id = ?",
        )
        .bind(rule.kind.as_str())
        .bind(&rule.label)
        .bind(&rule.body)
        .bind(match rule.cadence {
            BreakCadence::Interval { .. } => "interval",
            BreakCadence::Daily { .. } => "daily",
        })
        .bind(rule.cadence.interval_minutes().map(|m| m as i64))
        .bind(rule.cadence.at_time().map(|t| t.format("%H:%M").to_string()))
        .bind(rule.duration_seconds as i64)
        .bind(rule.priority as i64)
        .bind(i64::from(rule.enabled))
        .bind(rule.urgency.as_str())
        .bind(rule.updated_at.to_rfc3339())
        .bind(rule.user_id.to_string())
        .bind(rule.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, user_id: UserId, id: BreakRuleId) -> Result<(), RepositoryError> {
        // Explicit event cleanup rather than relying on ON DELETE CASCADE: SQLite only
        // enforces foreign keys when `PRAGMA foreign_keys` is on, and the pool's pragma
        // state is not this repository's to assume.
        sqlx::query("DELETE FROM break_events WHERE user_id = ? AND rule_id = ?")
            .bind(user_id.to_string())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        sqlx::query("DELETE FROM break_rules WHERE user_id = ? AND id = ?")
            .bind(user_id.to_string())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }
}

pub struct SqliteBreakEventRepository {
    pool: SqlitePool,
}

impl SqliteBreakEventRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BreakEventRepository for SqliteBreakEventRepository {
    async fn list_open(&self, user_id: UserId) -> Result<Vec<BreakEvent>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT * FROM break_events WHERE user_id = ? AND outcome = 'pending' \
             ORDER BY due_at ASC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter().map(map_event).collect()
    }

    async fn create(&self, event: &BreakEvent) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO break_events (id, user_id, rule_id, due_at, fired_at, deferred_until, \
             defer_reason, suppressed_by_meeting_id, outcome, responded_at, created_at) \
             VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(event.id.to_string())
        .bind(event.user_id.to_string())
        .bind(event.rule_id.to_string())
        .bind(event.due_at.to_rfc3339())
        .bind(event.fired_at.map(|d| d.to_rfc3339()))
        .bind(event.deferred_until.map(|d| d.to_rfc3339()))
        .bind(event.defer_reason.map(|r| r.as_str()))
        .bind(event.suppressed_by_meeting_id.as_deref())
        .bind(event.outcome.as_str())
        .bind(event.responded_at.map(|d| d.to_rfc3339()))
        .bind(event.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn set_outcome(
        &self,
        id: BreakEventId,
        outcome: BreakOutcome,
        responded_at: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE break_events SET outcome = ?, responded_at = ? WHERE id = ?")
            .bind(outcome.as_str())
            .bind(responded_at.map(|d| d.to_rfc3339()))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn set_deferral(
        &self,
        id: BreakEventId,
        until: DateTime<Utc>,
        reason: DeferReason,
        meeting_id: Option<&str>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE break_events SET deferred_until = ?, defer_reason = ?, \
             suppressed_by_meeting_id = ? WHERE id = ?",
        )
        .bind(until.to_rfc3339())
        .bind(reason.as_str())
        .bind(meeting_id)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn mark_fired(
        &self,
        id: BreakEventId,
        fired_at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE break_events SET fired_at = ?, deferred_until = NULL, defer_reason = NULL \
             WHERE id = ?",
        )
        .bind(fired_at.to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(())
    }

    async fn counts_between(
        &self,
        user_id: UserId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<(BreakRuleId, BreakOutcome, i64)>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT rule_id, outcome, COUNT(*) AS n FROM break_events \
             WHERE user_id = ? AND due_at >= ? AND due_at < ? GROUP BY rule_id, outcome",
        )
        .bind(user_id.to_string())
        .bind(from.to_rfc3339())
        .bind(to.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
        rows.iter()
            .map(|row| {
                let outcome_str: String = Row::get(row, "outcome");
                Ok((
                    parse_uuid(&Row::get::<String, _>(row, "rule_id"))?,
                    BreakOutcome::from_str(&outcome_str).ok_or_else(|| {
                        RepositoryError::Database(format!("bad outcome '{outcome_str}'"))
                    })?,
                    Row::get::<i64, _>(row, "n"),
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../../migrations/sqlite")
            .run(&pool)
            .await
            .unwrap();
        pool
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 27, h, m, 0).unwrap()
    }

    fn rule(user_id: UserId, cadence: BreakCadence) -> BreakRule {
        BreakRule {
            id: Uuid::new_v4(),
            user_id,
            kind: BreakKind::Posture,
            label: "Bouge".into(),
            body: "Lève-toi".into(),
            cadence,
            duration_seconds: 120,
            priority: 2,
            enabled: true,
            urgency: BreakUrgency::Normal,
            created_at: at(8, 0),
            updated_at: at(8, 0),
        }
    }

    #[tokio::test]
    async fn insert_then_list_round_trips_an_interval_rule() {
        let pool = pool().await;
        let repo = SqliteBreakRuleRepository::new(pool);
        let user_id = Uuid::new_v4();
        let r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        repo.create(&r).await.unwrap();
        let listed = repo.list(user_id).await.unwrap();
        assert_eq!(listed, vec![r]);
    }

    #[tokio::test]
    async fn insert_then_list_round_trips_a_daily_rule() {
        let pool = pool().await;
        let repo = SqliteBreakRuleRepository::new(pool);
        let user_id = Uuid::new_v4();
        let at_time = NaiveTime::from_hms_opt(14, 0, 0).unwrap();
        let r = rule(user_id, BreakCadence::Daily { at: at_time });
        repo.create(&r).await.unwrap();
        assert_eq!(repo.list(user_id).await.unwrap(), vec![r]);
    }

    #[tokio::test]
    async fn list_enabled_hides_disabled_rules() {
        let pool = pool().await;
        let repo = SqliteBreakRuleRepository::new(pool);
        let user_id = Uuid::new_v4();
        let mut off = rule(user_id, BreakCadence::Interval { minutes: 20 });
        off.enabled = false;
        repo.create(&off).await.unwrap();
        assert!(repo.list_enabled(user_id).await.unwrap().is_empty());
        assert_eq!(repo.list(user_id).await.unwrap().len(), 1);
    }

    /// The invariant the type system already enforces in memory must also hold in
    /// storage, because migrations and hand-edits bypass the type system entirely.
    #[tokio::test]
    async fn database_rejects_a_rule_carrying_both_cadence_shapes() {
        let pool = pool().await;
        let err = sqlx::query(
            "INSERT INTO break_rules (id, user_id, kind, label, body, cadence, interval_minutes,
                                      at_time, duration_seconds, priority, enabled, urgency,
                                      created_at, updated_at)
             VALUES (?, ?, 'posture', 'l', 'b', 'interval', 30, '14:00', 120, 1, 1, 'normal', ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(Uuid::new_v4().to_string())
        .bind(at(8, 0).to_rfc3339())
        .bind(at(8, 0).to_rfc3339())
        .execute(&pool)
        .await;
        assert!(err.is_err(), "CHECK must reject interval+at_time");
    }

    #[tokio::test]
    async fn update_replaces_every_editable_field() {
        let pool = pool().await;
        let repo = SqliteBreakRuleRepository::new(pool);
        let user_id = Uuid::new_v4();
        let mut r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        repo.create(&r).await.unwrap();
        r.label = "Autre".into();
        r.cadence = BreakCadence::Daily { at: NaiveTime::from_hms_opt(9, 30, 0).unwrap() };
        r.enabled = false;
        r.updated_at = at(9, 0);
        repo.update(&r).await.unwrap();
        assert_eq!(repo.list(user_id).await.unwrap(), vec![r]);
    }

    #[tokio::test]
    async fn deleting_a_rule_cascades_to_its_events() {
        let pool = pool().await;
        sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();
        let rules = SqliteBreakRuleRepository::new(pool.clone());
        let events = SqliteBreakEventRepository::new(pool.clone());
        let user_id = Uuid::new_v4();
        let r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        rules.create(&r).await.unwrap();
        let e = BreakEvent {
            id: Uuid::new_v4(),
            user_id,
            rule_id: r.id,
            due_at: at(9, 30),
            fired_at: None,
            deferred_until: None,
            defer_reason: None,
            suppressed_by_meeting_id: None,
            outcome: BreakOutcome::Pending,
            responded_at: None,
            created_at: at(9, 30),
        };
        events.create(&e).await.unwrap();
        rules.delete(user_id, r.id).await.unwrap();
        assert!(events.list_open(user_id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_open_returns_only_pending_events() {
        let pool = pool().await;
        let rules = SqliteBreakRuleRepository::new(pool.clone());
        let events = SqliteBreakEventRepository::new(pool.clone());
        let user_id = Uuid::new_v4();
        let r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        rules.create(&r).await.unwrap();
        let mut open = BreakEvent {
            id: Uuid::new_v4(),
            user_id,
            rule_id: r.id,
            due_at: at(9, 30),
            fired_at: None,
            deferred_until: Some(at(10, 0)),
            defer_reason: Some(DeferReason::Meeting),
            suppressed_by_meeting_id: Some("outlook-1".into()),
            outcome: BreakOutcome::Pending,
            responded_at: None,
            created_at: at(9, 30),
        };
        events.create(&open).await.unwrap();
        let mut done = open.clone();
        done.id = Uuid::new_v4();
        done.outcome = BreakOutcome::Taken;
        events.create(&done).await.unwrap();
        assert_eq!(events.list_open(user_id).await.unwrap(), vec![open.clone()]);

        open.outcome = BreakOutcome::Expired;
        events.set_outcome(open.id, BreakOutcome::Expired, None).await.unwrap();
        assert!(events.list_open(user_id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn counts_between_groups_by_rule_and_outcome() {
        let pool = pool().await;
        let rules = SqliteBreakRuleRepository::new(pool.clone());
        let events = SqliteBreakEventRepository::new(pool.clone());
        let user_id = Uuid::new_v4();
        let r = rule(user_id, BreakCadence::Interval { minutes: 30 });
        rules.create(&r).await.unwrap();
        for (n, outcome) in [(2, BreakOutcome::Taken), (1, BreakOutcome::Ignored)] {
            for _ in 0..n {
                events
                    .create(&BreakEvent {
                        id: Uuid::new_v4(),
                        user_id,
                        rule_id: r.id,
                        due_at: at(10, 0),
                        fired_at: Some(at(10, 0)),
                        deferred_until: None,
                        defer_reason: None,
                        suppressed_by_meeting_id: None,
                        outcome,
                        responded_at: Some(at(10, 1)),
                        created_at: at(10, 0),
                    })
                    .await
                    .unwrap();
            }
        }
        let counts = events.counts_between(user_id, at(0, 0), at(23, 59)).await.unwrap();
        assert_eq!(counts.len(), 2);
        assert!(counts.contains(&(r.id, BreakOutcome::Taken, 2)));
        assert!(counts.contains(&(r.id, BreakOutcome::Ignored, 1)));
    }

    /// Migration 019 seeds the default routine and 020 retunes the visual cadence, but
    /// nothing else verifies either: the GraphQL tests build their schema over in-memory
    /// repository fakes and never run a migration, so a real SQLite pool running the real
    /// migrations is the only place this seed can be checked at all.
    ///
    /// The visual rule is asserted at 15 rather than the 20 that 019 seeded: 20/30/60
    /// interleave (dues at :20, :30, :40, :00), where 15/30/60 coincide at :30 and :00
    /// and give an even quarter-hour rhythm.
    #[tokio::test]
    async fn migration_seeds_the_default_break_routine() {
        let pool = pool().await;
        let repo = SqliteBreakRuleRepository::new(pool);
        let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let rules = repo.list(user_id).await.unwrap();
        assert_eq!(rules.len(), 4);
        assert!(rules.iter().all(|r| r.enabled));
        assert_eq!(
            rules.iter().map(|r| r.cadence).collect::<Vec<_>>(),
            vec![
                BreakCadence::Interval { minutes: 15 },
                BreakCadence::Interval { minutes: 30 },
                BreakCadence::Interval { minutes: 60 },
                BreakCadence::Daily { at: NaiveTime::from_hms_opt(14, 0, 0).unwrap() },
            ]
        );
        assert_eq!(rules[3].kind, BreakKind::Strength);

        // 020 is targeted at the seeded id and must leave the rest of the row alone;
        // it also bumps `updated_at`, which is how a retune is distinguishable from
        // the untouched seed.
        let visual = &rules[0];
        assert_eq!(visual.id, Uuid::parse_str("11111111-1111-4111-8111-000000000001").unwrap());
        assert_eq!(visual.kind, BreakKind::Visual);
        assert_eq!(visual.duration_seconds, 30);
        assert_eq!(visual.priority, 1);
        assert!(visual.updated_at > visual.created_at);
        assert!(!visual.allows_snooze(), "a quarter-hour cadence offers no deferral");
    }
}
