use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tokio::net::TcpListener;

mod context;
mod graphql;
mod middleware;
mod state;

use infrastructure::database::*;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:aggregated_plan.db?mode=rwc".to_string());
    let db_pool = create_sqlite_pool(&database_url).await.unwrap();

    // Build repository instances
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
    let task_link_repo: Arc<dyn application::repositories::TaskLinkRepository> =
        Arc::new(SqliteTaskLinkRepository::new(db_pool.clone()));
    let sync_repo: Arc<dyn application::repositories::SyncStatusRepository> =
        Arc::new(SqliteSyncStatusRepository::new(db_pool.clone()));
    let config_repo: Arc<dyn application::repositories::ConfigRepository> =
        Arc::new(SqliteConfigRepository::new(db_pool.clone()));

    let schema = graphql::schema::build_schema(
        task_repo,
        meeting_repo,
        project_repo,
        activity_repo,
        alert_repo,
        tag_repo,
        task_link_repo,
        sync_repo,
        config_repo,
    );

    let app = Router::new()
        .route("/graphql", post(graphql::schema::graphql_handler))
        .route("/graphql/playground", get(graphql::schema::graphql_playground))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state::AppState {
            schema: schema.clone(),
        });

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    tracing::info!("Server running on http://{}", addr);
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
