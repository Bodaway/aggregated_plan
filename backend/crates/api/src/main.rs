use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

mod auth;
mod graphql;
mod state;

use uuid::Uuid;

use graphql::schema::SchemaDeps;
use infrastructure::database::*;
use infrastructure::connectors::microsoft::oauth::{MicrosoftOAuth, MicrosoftOAuthConfig};
use infrastructure::connectors::microsoft::token_provider::RefreshingGraphTokenProvider;

#[derive(Parser)]
#[command(name = "api", about = "Aggregated Plan API server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print the GraphQL SDL to stdout and exit (used by the CLI codegen).
    ExportSchema,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

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
    let worklog_repo: Arc<dyn application::repositories::WorklogRepository> =
        Arc::new(SqliteWorklogRepository::new(db_pool.clone()));
    let recurrence_repo: Arc<dyn application::repositories::RecurrenceRepository> =
        Arc::new(SqliteRecurrenceRepository::new(db_pool.clone()));

    let oauth = std::sync::Arc::new(MicrosoftOAuth::new(MicrosoftOAuthConfig {
        client_id: std::env::var("MICROSOFT_CLIENT_ID").unwrap_or_default(),
        tenant_id: std::env::var("MICROSOFT_TENANT_ID").unwrap_or_default(),
        client_secret: std::env::var("MICROSOFT_CLIENT_SECRET").unwrap_or_default(),
        redirect_uri: std::env::var("MICROSOFT_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:3001/auth/microsoft/callback".to_string()),
    }));
    let graph_token_provider: std::sync::Arc<dyn application::services::GraphTokenProvider> =
        std::sync::Arc::new(RefreshingGraphTokenProvider::new(config_repo.clone(), oauth.clone()));

    let deps = SchemaDeps {
        task_repo,
        meeting_repo,
        project_repo,
        activity_repo,
        alert_repo,
        tag_repo,
        task_link_repo,
        sync_repo,
        config_repo: config_repo.clone(),
        worklog_repo,
        recurrence_repo,
        graph_token_provider: graph_token_provider.clone(),
    };
    let schema = graphql::schema::build_schema(deps);

    if let Some(Command::ExportSchema) = cli.command {
        println!("{}", schema.sdl());
        return;
    }

    let default_user_id =
        Uuid::parse_str(state::DEFAULT_USER_ID_STR).unwrap();

    let app = Router::new()
        .route("/graphql", post(graphql::schema::graphql_handler))
        .route("/graphql/playground", get(graphql::schema::graphql_playground))
        .route("/auth/microsoft/login", get(auth::microsoft::login))
        .route("/auth/microsoft/callback", get(auth::microsoft::callback))
        .layer(
            CorsLayer::new()
                .allow_origin("http://localhost:3000".parse::<axum::http::HeaderValue>().unwrap())
                .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
                .allow_headers([axum::http::header::CONTENT_TYPE]),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state::AppState {
            schema: schema.clone(),
            config_repo: config_repo.clone(),
            oauth: oauth.clone(),
            default_user_id,
            oauth_state: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        });

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    tracing::info!("Server running on http://{}", addr);
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
