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
    use sqlx::SqlitePool;

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
}
