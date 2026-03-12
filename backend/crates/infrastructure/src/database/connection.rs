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
