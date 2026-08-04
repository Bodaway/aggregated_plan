use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

/// Create a SQLite connection pool and run pending migrations.
pub async fn create_sqlite_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("../../../migrations/sqlite")
        .run(&pool)
        .await?;

    // Seed the default local user if it does not exist yet.
    sqlx::query(
        "INSERT OR IGNORE INTO users (id, name, email) VALUES (?, ?, ?)"
    )
    .bind("00000000-0000-0000-0000-000000000001")
    .bind("Local User")
    .bind("local@aggregated-plan.local")
    .execute(&pool)
    .await?;

    Ok(pool)
}

#[cfg(test)]
mod migration_tests {
    use super::create_sqlite_pool;
    use sqlx::SqlitePool;
    use std::borrow::Cow;

    #[tokio::test]
    async fn migrations_create_timesheet_and_mapping_tables() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../../migrations/sqlite")
            .run(&pool)
            .await
            .unwrap();

        for table in [
            "timesheet_drafts",
            "timesheet_draft_lines",
            "signal_project_mappings",
        ] {
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(row.0, 1, "table {table} should exist after migration");
        }
    }

    /// Step 10 of the table rebuild 013 performs on `alerts`. SQL cannot fail a
    /// migration on it — `PRAGMA foreign_key_check` returns rows rather than
    /// raising — so the assertion has to live here.
    #[tokio::test]
    async fn the_migrated_schema_has_no_broken_foreign_key() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let violations: Vec<(String,)> =
            sqlx::query_as("SELECT \"table\" FROM pragma_foreign_key_check")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(
            violations.is_empty(),
            "foreign_key_check reported {violations:?}"
        );
    }

    /// Rebuilding a table drops its indexes with it. 013 recreates 001's index by
    /// hand, and forgetting that turns every `find_unresolved` into a table scan
    /// without a single test going red.
    #[tokio::test]
    async fn the_alerts_rebuild_keeps_its_index() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='index' AND name='idx_alerts_user_resolved' AND tbl_name='alerts'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 1, "013 must recreate the index it dropped");
    }

    /// The property a table rebuild has to prove: the rows that were there before
    /// are still there after, unchanged.
    ///
    /// Migrations are applied in two passes around the insert — 001..012, seed,
    /// then 013 alone — because a pool that runs everything at once can never hold
    /// a row written under the old table.
    #[tokio::test]
    async fn the_alerts_rebuild_preserves_every_row() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let mut migrator = sqlx::migrate!("../../../migrations/sqlite");
        let all = migrator.migrations.to_vec();
        assert!(
            all.iter().any(|m| m.version == 13),
            "013 must be part of the embedded set"
        );

        migrator.migrations = Cow::Owned(all.iter().filter(|m| m.version < 13).cloned().collect());
        migrator.run(&pool).await.expect("001..012 apply");

        sqlx::query(
            "INSERT INTO users (id, name, email, created_at)
             VALUES ('00000000-0000-0000-0000-000000000001', 'T', 't@example.test', '2026-08-03T09:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, alert_type) in [("a1", "deadline"), ("a2", "overload"), ("a3", "conflict")] {
            sqlx::query(
                "INSERT INTO alerts (id, user_id, alert_type, severity, message, related_items, date, resolved, created_at)
                 VALUES (?, '00000000-0000-0000-0000-000000000001', ?, 'warning', ?, '[]', '2026-08-03', 0, '2026-08-03T09:00:00+00:00')",
            )
            .bind(id)
            .bind(alert_type)
            .bind(format!("message for {id}"))
            .execute(&pool)
            .await
            .unwrap();
        }
        let before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM alerts")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before.0, 3);

        // `ignore_missing`: this second pass knows only 013, while the ledger
        // already records 001..012.
        migrator.ignore_missing = true;
        migrator.migrations = Cow::Owned(all.iter().filter(|m| m.version == 13).cloned().collect());
        migrator.run(&pool).await.expect("013 applies");

        let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT id, alert_type, message, resolved FROM alerts ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 3, "the rebuild must not lose a row");
        assert_eq!(
            rows,
            vec![
                ("a1".into(), "deadline".into(), "message for a1".into(), 0),
                ("a2".into(), "overload".into(), "message for a2".into(), 0),
                ("a3".into(), "conflict".into(), "message for a3".into(), 0),
            ]
        );

        // And the column the rebuild existed for now accepts the fourth variant.
        sqlx::query(
            "INSERT INTO alerts (id, user_id, alert_type, severity, message, related_items, date, resolved, created_at)
             VALUES ('a4', '00000000-0000-0000-0000-000000000001', 'timesheet_ready', 'information', 'draft ready', '[]', '2026-08-03', 0, '2026-08-03T18:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .expect("timesheet_ready must be storable after 013");
    }
}
