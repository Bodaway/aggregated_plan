use std::sync::Arc;

use infrastructure::database::*;

mod server;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    // Tracing to stderr — stdout is reserved for the MCP protocol
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:aggregated_plan.db?mode=rwc".to_string());
    let db_pool = create_sqlite_pool(&database_url)
        .await
        .expect("Failed to create database pool");

    let task_repo: Arc<dyn application::repositories::TaskRepository> =
        Arc::new(SqliteTaskRepository::new(db_pool.clone()));
    let meeting_repo: Arc<dyn application::repositories::MeetingRepository> =
        Arc::new(SqliteMeetingRepository::new(db_pool.clone()));
    let project_repo: Arc<dyn application::repositories::ProjectRepository> =
        Arc::new(SqliteProjectRepository::new(db_pool.clone()));
    let activity_repo: Arc<dyn application::repositories::ActivitySlotRepository> =
        Arc::new(SqliteActivitySlotRepository::new(db_pool.clone()));
    let alert_repo: Arc<dyn application::repositories::AlertRepository> =
        Arc::new(SqliteAlertRepository::new(db_pool.clone()));
    let tag_repo: Arc<dyn application::repositories::TagRepository> =
        Arc::new(SqliteTagRepository::new(db_pool.clone()));
    let sync_repo: Arc<dyn application::repositories::SyncStatusRepository> =
        Arc::new(SqliteSyncStatusRepository::new(db_pool.clone()));
    let config_repo: Arc<dyn application::repositories::ConfigRepository> =
        Arc::new(SqliteConfigRepository::new(db_pool.clone()));

    let user_id =
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let server = server::AggregatedPlanServer::new(
        task_repo,
        meeting_repo,
        project_repo,
        activity_repo,
        alert_repo,
        tag_repo,
        sync_repo,
        config_repo,
        user_id,
    );

    tracing::info!("Starting Aggregated Plan MCP server on stdio");

    let transport = rmcp::transport::io::stdio();
    let server = rmcp::ServiceExt::serve(server, transport)
        .await
        .expect("Failed to start MCP server");
    server.waiting().await.expect("MCP server error");
}
