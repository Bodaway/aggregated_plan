use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use application::errors::RepositoryError;
use application::repositories::RecurrenceRepository;
use domain::types::common::{ProjectId, TagId, UserId};
use domain::types::recurrence::{RecurrenceRule, RecurrenceTemplate, RecurrenceTemplateId};

use super::conversions::{impact_from_i32, impact_to_i32, urgency_from_i32, urgency_to_i32};

pub struct SqliteRecurrenceRepository {
    pool: SqlitePool,
}

impl SqliteRecurrenceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn parse_datetime_rfc3339(s: &str) -> Result<DateTime<Utc>, RepositoryError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
        })
        .map_err(|e| RepositoryError::Database(format!("Failed to parse datetime '{}': {}", s, e)))
}

fn parse_optional_date(s: Option<String>) -> Result<Option<NaiveDate>, RepositoryError> {
    match s {
        Some(ref val) if !val.is_empty() => NaiveDate::parse_from_str(val, "%Y-%m-%d")
            .map(Some)
            .map_err(|e| RepositoryError::Database(format!("Failed to parse date '{}': {}", val, e))),
        _ => Ok(None),
    }
}

fn map_template_row(row: &SqliteRow) -> Result<RecurrenceTemplate, RepositoryError> {
    let id_str: String = Row::get(row, "id");
    let user_id_str: String = Row::get(row, "user_id");
    let urgency_val: i32 = Row::get(row, "urgency");
    let urgency_manual_val: i32 = Row::get(row, "urgency_manual");
    let impact_val: i32 = Row::get(row, "impact");
    let estimated_hours: Option<f64> = Row::get(row, "estimated_hours");
    let rule_json: String = Row::get(row, "rule_json");
    let starts_on_str: String = Row::get(row, "starts_on");
    let ends_on_str: Option<String> = Row::get(row, "ends_on");
    let last_generated_str: Option<String> = Row::get(row, "last_generated_through");
    let active_val: i32 = Row::get(row, "active");
    let created_at_str: String = Row::get(row, "created_at");
    let updated_at_str: String = Row::get(row, "updated_at");
    let project_id_str: Option<String> = Row::get(row, "project_id");

    let project_id: Option<ProjectId> = match project_id_str {
        Some(ref s) if !s.is_empty() => Some(
            Uuid::parse_str(s).map_err(|e| RepositoryError::Database(e.to_string()))?,
        ),
        _ => None,
    };

    let rule: RecurrenceRule = serde_json::from_str(&rule_json)
        .map_err(|e| RepositoryError::Database(format!("Failed to deserialize rule_json: {}", e)))?;

    let starts_on = NaiveDate::parse_from_str(&starts_on_str, "%Y-%m-%d")
        .map_err(|e| RepositoryError::Database(format!("Failed to parse starts_on '{}': {}", starts_on_str, e)))?;

    Ok(RecurrenceTemplate {
        id: RecurrenceTemplateId::from_uuid(
            Uuid::parse_str(&id_str).map_err(|e| RepositoryError::Database(e.to_string()))?,
        ),
        user_id: Uuid::parse_str(&user_id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?,
        title: Row::get(row, "title"),
        description: Row::get(row, "description"),
        notes: Row::get(row, "notes"),
        project_id,
        urgency: urgency_from_i32(urgency_val),
        urgency_manual: urgency_manual_val != 0,
        impact: impact_from_i32(impact_val),
        estimated_hours: estimated_hours.map(|v| v as f32),
        tags: Vec::new(), // loaded separately
        rule,
        starts_on,
        ends_on: parse_optional_date(ends_on_str)?,
        max_occurrences: {
            let v: Option<i64> = Row::get(row, "max_occurrences");
            v.map(|n| n as u32)
        },
        last_generated_through: parse_optional_date(last_generated_str)?,
        active: active_val != 0,
        created_at: parse_datetime_rfc3339(&created_at_str)?,
        updated_at: parse_datetime_rfc3339(&updated_at_str)?,
    })
}

async fn load_tags_for_template(
    pool: &SqlitePool,
    template_id: &RecurrenceTemplateId,
) -> Result<Vec<TagId>, RepositoryError> {
    let rows = sqlx::query(
        "SELECT tag_id FROM task_recurrence_tags WHERE template_id = ?",
    )
    .bind(template_id.to_string())
    .fetch_all(pool)
    .await
    .map_err(|e| RepositoryError::Database(e.to_string()))?;

    rows.iter()
        .map(|row| {
            let tag_id_str: String = Row::get(row, "tag_id");
            Uuid::parse_str(&tag_id_str).map_err(|e| RepositoryError::Database(e.to_string()))
        })
        .collect()
}

async fn save_template_tags(
    pool: &SqlitePool,
    template_id: &RecurrenceTemplateId,
    tags: &[TagId],
) -> Result<(), RepositoryError> {
    sqlx::query("DELETE FROM task_recurrence_tags WHERE template_id = ?")
        .bind(template_id.to_string())
        .execute(pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

    for tag_id in tags {
        sqlx::query(
            "INSERT INTO task_recurrence_tags (template_id, tag_id) VALUES (?, ?)",
        )
        .bind(template_id.to_string())
        .bind(tag_id.to_string())
        .execute(pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
    }

    Ok(())
}

#[async_trait]
impl RecurrenceRepository for SqliteRecurrenceRepository {
    async fn find_by_id(
        &self,
        id: RecurrenceTemplateId,
    ) -> Result<Option<RecurrenceTemplate>, RepositoryError> {
        let rows = sqlx::query("SELECT * FROM task_recurrences WHERE id = ?")
            .bind(id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        match rows.first() {
            Some(row) => {
                let mut template = map_template_row(row)?;
                template.tags = load_tags_for_template(&self.pool, &template.id).await?;
                Ok(Some(template))
            }
            None => Ok(None),
        }
    }

    async fn find_active_by_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<RecurrenceTemplate>, RepositoryError> {
        let rows =
            sqlx::query("SELECT * FROM task_recurrences WHERE user_id = ? AND active = 1")
                .bind(user_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let mut templates: Vec<RecurrenceTemplate> =
            rows.iter().map(map_template_row).collect::<Result<_, _>>()?;

        // Load tags per template (one extra query per template — acceptable for MVP).
        for t in templates.iter_mut() {
            t.tags = load_tags_for_template(&self.pool, &t.id).await?;
        }

        Ok(templates)
    }

    async fn save(&self, template: &RecurrenceTemplate) -> Result<(), RepositoryError> {
        let rule_json = serde_json::to_string(&template.rule)
            .map_err(|e| RepositoryError::Database(format!("Failed to serialize rule: {}", e)))?;

        sqlx::query(
            "INSERT OR REPLACE INTO task_recurrences \
             (id, user_id, title, description, notes, project_id, urgency, urgency_manual, \
              impact, estimated_hours, rule_json, starts_on, ends_on, max_occurrences, \
              last_generated_through, active, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(template.id.to_string())
        .bind(template.user_id.to_string())
        .bind(&template.title)
        .bind(&template.description)
        .bind(&template.notes)
        .bind(template.project_id.map(|id| id.to_string()))
        .bind(urgency_to_i32(template.urgency))
        .bind(if template.urgency_manual { 1i32 } else { 0i32 })
        .bind(impact_to_i32(template.impact))
        .bind(template.estimated_hours.map(|h| h as f64))
        .bind(&rule_json)
        .bind(template.starts_on.format("%Y-%m-%d").to_string())
        .bind(template.ends_on.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(template.max_occurrences.map(|n| n as i64))
        .bind(template.last_generated_through.map(|d| d.format("%Y-%m-%d").to_string()))
        .bind(if template.active { 1i32 } else { 0i32 })
        .bind(template.created_at.to_rfc3339())
        .bind(template.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        save_template_tags(&self.pool, &template.id, &template.tags).await?;

        Ok(())
    }

    async fn deactivate(&self, id: RecurrenceTemplateId) -> Result<(), RepositoryError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE task_recurrences SET active = 0, updated_at = ? WHERE id = ?",
        )
        .bind(&now)
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
    use domain::types::recurrence::{RecurrenceRule, WeekOfMonth, WeekdaySet};
    use chrono::Weekday;
    use domain::types::common::{ImpactLevel, UrgencyLevel};

    async fn setup() -> SqlitePool {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query(
            "INSERT OR IGNORE INTO users (id, name, email, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind("00000000-0000-0000-0000-000000000001")
        .bind("Test User")
        .bind("test@example.com")
        .bind("2024-01-01T00:00:00+00:00")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn user_id() -> UserId {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn make_template(rule: RecurrenceRule) -> RecurrenceTemplate {
        RecurrenceTemplate {
            id: RecurrenceTemplateId::new(),
            user_id: user_id(),
            title: "Test template".to_string(),
            description: None,
            notes: None,
            project_id: None,
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            estimated_hours: None,
            tags: Vec::new(),
            rule,
            starts_on: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            ends_on: None,
            max_occurrences: None,
            last_generated_through: None,
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    // Test 1: Round-trip Daily { interval: 1 }
    #[tokio::test]
    async fn round_trip_daily() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool);
        let template = make_template(RecurrenceRule::Daily { interval: 1 });

        repo.save(&template).await.unwrap();
        let found = repo.find_by_id(template.id).await.unwrap().unwrap();

        assert_eq!(found.id, template.id);
        assert_eq!(found.title, template.title);
        assert_eq!(found.rule, RecurrenceRule::Daily { interval: 1 });
        assert_eq!(found.starts_on, template.starts_on);
        assert!(found.active);
    }

    // Test 2: Round-trip Weekly { interval: 2, weekdays: Mon+Wed+Fri }
    #[tokio::test]
    async fn round_trip_weekly() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool);

        let mut weekdays = WeekdaySet::empty();
        weekdays.insert(Weekday::Mon);
        weekdays.insert(Weekday::Wed);
        weekdays.insert(Weekday::Fri);

        let template = make_template(RecurrenceRule::Weekly { interval: 2, weekdays });

        repo.save(&template).await.unwrap();
        let found = repo.find_by_id(template.id).await.unwrap().unwrap();

        assert_eq!(found.rule, RecurrenceRule::Weekly { interval: 2, weekdays });
    }

    // Test 3: Round-trip MonthlyByDay { interval: 1, day: 31 }
    #[tokio::test]
    async fn round_trip_monthly_by_day() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool);
        let template = make_template(RecurrenceRule::MonthlyByDay { interval: 1, day: 31 });

        repo.save(&template).await.unwrap();
        let found = repo.find_by_id(template.id).await.unwrap().unwrap();

        assert_eq!(found.rule, RecurrenceRule::MonthlyByDay { interval: 1, day: 31 });
    }

    // Test 4: Round-trip MonthlyByWeekday { interval: 1, week: First, weekday: Tuesday }
    #[tokio::test]
    async fn round_trip_monthly_by_weekday() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool);
        let template = make_template(RecurrenceRule::MonthlyByWeekday {
            interval: 1,
            week: WeekOfMonth::First,
            weekday: Weekday::Tue,
        });

        repo.save(&template).await.unwrap();
        let found = repo.find_by_id(template.id).await.unwrap().unwrap();

        assert_eq!(
            found.rule,
            RecurrenceRule::MonthlyByWeekday {
                interval: 1,
                week: WeekOfMonth::First,
                weekday: Weekday::Tue,
            }
        );
    }

    // Test 5: find_active_by_user excludes deactivated templates
    #[tokio::test]
    async fn find_active_by_user_excludes_inactive() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool);

        let active = make_template(RecurrenceRule::Daily { interval: 1 });
        let mut inactive = make_template(RecurrenceRule::Daily { interval: 2 });
        inactive.title = "Inactive".to_string();

        repo.save(&active).await.unwrap();
        repo.save(&inactive).await.unwrap();

        // Deactivate the second
        repo.deactivate(inactive.id).await.unwrap();

        let results = repo.find_active_by_user(user_id()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, active.id);
    }

    // Test 6: Tags persist through save and are returned by find_by_id
    #[tokio::test]
    async fn tags_persist_through_save() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool.clone());

        // Insert two tags into the tags table first (names must be unique per user)
        let tag_a = Uuid::new_v4();
        let tag_b = Uuid::new_v4();
        sqlx::query("INSERT INTO tags (id, user_id, name) VALUES (?, ?, ?)")
            .bind(tag_a.to_string())
            .bind(user_id().to_string())
            .bind("tag-alpha")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tags (id, user_id, name) VALUES (?, ?, ?)")
            .bind(tag_b.to_string())
            .bind(user_id().to_string())
            .bind("tag-beta")
            .execute(&pool)
            .await
            .unwrap();

        let mut template = make_template(RecurrenceRule::Daily { interval: 1 });
        template.tags = vec![tag_a, tag_b];

        repo.save(&template).await.unwrap();
        let found = repo.find_by_id(template.id).await.unwrap().unwrap();

        assert_eq!(found.tags.len(), 2);
        assert!(found.tags.contains(&tag_a));
        assert!(found.tags.contains(&tag_b));
    }

    // Test 7: deactivate sets active = 0
    #[tokio::test]
    async fn deactivate_sets_active_false() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool);
        let template = make_template(RecurrenceRule::Daily { interval: 1 });

        repo.save(&template).await.unwrap();
        assert!(repo.find_by_id(template.id).await.unwrap().unwrap().active);

        repo.deactivate(template.id).await.unwrap();
        assert!(!repo.find_by_id(template.id).await.unwrap().unwrap().active);
    }

    // Test: find_by_id returns None for missing id
    #[tokio::test]
    async fn find_by_id_not_found() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool);
        let result = repo.find_by_id(RecurrenceTemplateId::new()).await.unwrap();
        assert!(result.is_none());
    }

    // Test: optional fields (ends_on, max_occurrences, estimated_hours, description, notes)
    #[tokio::test]
    async fn optional_fields_round_trip() {
        let pool = setup().await;
        let repo = SqliteRecurrenceRepository::new(pool);

        let mut template = make_template(RecurrenceRule::Daily { interval: 7 });
        template.description = Some("Weekly task".to_string());
        template.notes = Some("# Notes".to_string());
        template.estimated_hours = Some(2.5);
        template.ends_on = Some(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
        template.max_occurrences = Some(52);

        repo.save(&template).await.unwrap();
        let found = repo.find_by_id(template.id).await.unwrap().unwrap();

        assert_eq!(found.description.as_deref(), Some("Weekly task"));
        assert_eq!(found.notes.as_deref(), Some("# Notes"));
        assert_eq!(found.estimated_hours, Some(2.5));
        assert_eq!(found.ends_on, Some(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()));
        assert_eq!(found.max_occurrences, Some(52));
    }
}
