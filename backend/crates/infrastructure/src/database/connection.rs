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

    /// Every `domain::Source` must be storable in `sync_status.source`.
    ///
    /// 001 enumerated four sources; the enum has six. `sync_gryzzly` writes
    /// `sync_status(gryzzly) -> syncing` as its first step, so the missing value made
    /// `aplan sync --source gryzzly` fail with `(code: 275) CHECK constraint failed`
    /// before the connector was even reached — the source had never run once. 015
    /// widens the CHECK; this test is what makes the NEXT added variant go red here
    /// instead of in production.
    #[tokio::test]
    async fn sync_status_accepts_every_source_variant() {
        use domain::types::Source;

        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO users (id, email, name) VALUES ('u1', 'u@example.com', 'U')")
            .execute(&pool)
            .await
            .unwrap();

        // Exhaustive by construction: adding a variant to `Source` without adding it
        // here is a compile error, and adding it here without widening the CHECK
        // fails the assertion below.
        let all = [
            Source::Jira,
            Source::Excel,
            Source::Obsidian,
            Source::Personal,
            Source::Outlook,
            Source::Gryzzly,
        ];
        for source in all {
            let as_str = crate::database::conversions::source_to_str(source);
            let res = sqlx::query(
                "INSERT INTO sync_status (id, user_id, source, status) VALUES (?, 'u1', ?, 'idle')",
            )
            .bind(format!("s-{as_str}"))
            .bind(as_str)
            .execute(&pool)
            .await;
            assert!(
                res.is_ok(),
                "sync_status.source rejects {as_str:?}: {:?}",
                res.err()
            );
        }
    }

    /// Sibling of `sync_status_accepts_every_source_variant`. That test guards the
    /// `source` column only — `status` carries the same enumerated-CHECK trap, and
    /// this codebase has now hit that trap three times (alerts.alert_type in 013,
    /// sync_status.source in 015, sync_status.status in 016). Both tests must exist
    /// or the next added variant ships broken.
    #[tokio::test]
    async fn sync_status_accepts_every_status_variant() {
        use domain::types::SyncSourceStatus;

        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO users (id, email, name) VALUES ('u1', 'u@example.com', 'U')")
            .execute(&pool)
            .await
            .unwrap();

        // Exhaustive by construction: adding a variant without listing it here is a
        // compile error, and listing it without widening the CHECK fails below.
        let all = [
            SyncSourceStatus::Idle,
            SyncSourceStatus::Syncing,
            SyncSourceStatus::Success,
            SyncSourceStatus::Error,
            SyncSourceStatus::NotConfigured,
        ];
        for (i, status) in all.into_iter().enumerate() {
            let as_str = crate::database::conversions::sync_status_to_str(status);
            let res = sqlx::query(
                "INSERT INTO sync_status (id, user_id, source, status) VALUES (?, 'u1', ?, ?)",
            )
            .bind(format!("st-{i}"))
            // A distinct source per row: (user_id, source) is UNIQUE.
            .bind(["jira", "outlook", "excel", "obsidian", "personal"][i])
            .bind(as_str)
            .execute(&pool)
            .await;
            assert!(
                res.is_ok(),
                "sync_status.status rejects {as_str:?}: {:?}",
                res.err()
            );
        }
    }

    /// The new column must exist and default to NULL, since NULL is the documented
    /// "unknown, treat as active" state for rows predating it.
    #[tokio::test]
    async fn gryzzly_tasks_has_a_nullable_project_status() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM pragma_table_info('gryzzly_tasks') WHERE name = 'project_status'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 1, "project_status column missing");

        let notnull: (i64,) = sqlx::query_as(
            "SELECT \"notnull\" FROM pragma_table_info('gryzzly_tasks') WHERE name = 'project_status'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(notnull.0, 0, "project_status must be nullable");
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

    #[tokio::test]
    async fn migrations_create_the_sessions_table() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 1, "014 must create the sessions table");
    }

    #[tokio::test]
    async fn migrations_add_the_authorship_and_provenance_columns() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        for (table, column) in [
            ("worklog_entries", "session_id"),
            ("activity_slots", "session_id"),
            ("activity_slots", "source"),
        ] {
            let names: Vec<(String,)> =
                sqlx::query_as("SELECT name FROM pragma_table_info(?)")
                    .bind(table)
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert!(
                names.iter().any(|(n,)| n == column),
                "{table}.{column} should exist after 014"
            );
        }
    }

    /// `mode` is the one column a wrong write would make meaningless: a session
    /// neither tracking nor off has no defined behaviour, so the store refuses it.
    #[tokio::test]
    async fn the_sessions_table_rejects_an_unknown_mode() {
        let pool = create_sqlite_pool("sqlite::memory:").await.unwrap();
        let result = sqlx::query(
            "INSERT INTO sessions (id, user_id, mode, started_at, last_seen_at)
             VALUES ('s1', '00000000-0000-0000-0000-000000000001', 'maybe',
                     '2026-08-04T09:00:00+00:00', '2026-08-04T09:00:00+00:00')",
        )
        .execute(&pool)
        .await;
        assert!(result.is_err(), "the CHECK on mode must reject `maybe`");
    }

    /// Adding a column to a populated table is the failure mode of an `ALTER`
    /// that SQLite would rather reject than migrate: the existing rows must all
    /// still be there, with the new columns null.
    #[tokio::test]
    async fn the_new_columns_leave_existing_rows_untouched() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let mut migrator = sqlx::migrate!("../../../migrations/sqlite");
        let all = migrator.migrations.to_vec();
        assert!(
            all.iter().any(|m| m.version == 14),
            "014 must be part of the embedded set"
        );

        migrator.migrations = Cow::Owned(all.iter().filter(|m| m.version < 14).cloned().collect());
        migrator.run(&pool).await.expect("001..013 apply");

        sqlx::query(
            "INSERT INTO users (id, name, email, created_at)
             VALUES ('00000000-0000-0000-0000-000000000001', 'T', 't@example.test', '2026-08-04T09:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks (id, user_id, title, source, status, urgency, impact, created_at, updated_at)
             VALUES ('t1', '00000000-0000-0000-0000-000000000001', 'Tâche', 'personal', 'todo', 2, 2,
                     '2026-08-04T09:00:00+00:00', '2026-08-04T09:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO activity_slots (id, user_id, task_id, start_time, end_time, half_day, date, created_at)
             VALUES ('sl1', '00000000-0000-0000-0000-000000000001', 't1',
                     '2026-08-04T09:00:00+00:00', '2026-08-04T11:00:00+00:00', 'morning',
                     '2026-08-04', '2026-08-04T11:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        migrator.ignore_missing = true;
        migrator.migrations = Cow::Owned(all.iter().filter(|m| m.version == 14).cloned().collect());
        migrator.run(&pool).await.expect("014 applies");

        let rows: Vec<(String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT id, session_id, source FROM activity_slots ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![("sl1".to_string(), None, None)],
            "the row survives and its new columns are null"
        );
    }
}
