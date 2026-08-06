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
mod jobs;
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
    let session_repo: Arc<dyn application::repositories::SessionRepository> =
        Arc::new(SqliteSessionRepository::new(db_pool.clone()));

    let default_user_id = Uuid::parse_str(state::DEFAULT_USER_ID_STR).unwrap();

    let recurrence_repo: Arc<dyn application::repositories::RecurrenceRepository> =
        Arc::new(SqliteRecurrenceRepository::new(db_pool.clone()));
    let gryzzly_catalog_repo: Arc<dyn application::repositories::GryzzlyCatalogRepository> =
        Arc::new(SqliteGryzzlyCatalogRepository::new(db_pool.clone()));
    let timesheet_draft_repo: Arc<dyn application::repositories::TimesheetDraftRepository> =
        Arc::new(SqliteTimesheetDraftRepository::new(db_pool.clone()));
    let signal_mapping_repo: Arc<dyn application::repositories::SignalMappingRepository> =
        Arc::new(SqliteSignalMappingRepository::new(db_pool.clone()));
    let memory_repo: Arc<dyn application::repositories::MemoryRepository> =
        Arc::new(SqliteMemoryRepository::new(db_pool.clone()));
    let memory_retriever: Arc<dyn application::services::MemoryRetriever> =
        Arc::new(SqliteMemoryRetriever::new(db_pool.clone()));
    let memory_file_source: Arc<dyn application::services::MemoryFileSource> = Arc::new(
        infrastructure::connectors::memory_files::FsMemoryFileSource::new(),
    );
    let git_connector: Arc<dyn application::services::git_connector::GitConnector> =
        Arc::new(infrastructure::connectors::git::ShellGitConnector::new());

    let oauth = std::sync::Arc::new(MicrosoftOAuth::new(MicrosoftOAuthConfig {
        client_id: std::env::var("MICROSOFT_CLIENT_ID").unwrap_or_default(),
        tenant_id: std::env::var("MICROSOFT_TENANT_ID").unwrap_or_default(),
        client_secret: std::env::var("MICROSOFT_CLIENT_SECRET").unwrap_or_default(),
        redirect_uri: std::env::var("MICROSOFT_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:3001/auth/microsoft/callback".to_string()),
    }));
    let graph_token_provider: std::sync::Arc<dyn application::services::GraphTokenProvider> =
        std::sync::Arc::new(RefreshingGraphTokenProvider::new(config_repo.clone(), oauth.clone()));

    let eod_deps = jobs::EodDeps {
        worklog_repo: worklog_repo.clone(),
        meeting_repo: meeting_repo.clone(),
        task_repo: task_repo.clone(),
        catalog_repo: gryzzly_catalog_repo.clone(),
        mapping_repo: signal_mapping_repo.clone(),
        config_repo: config_repo.clone(),
        git: git_connector.clone(),
        draft_repo: timesheet_draft_repo.clone(),
        alert_repo: alert_repo.clone(),
    };

    let deps = SchemaDeps {
        task_repo,
        meeting_repo,
        project_repo,
        activity_repo: activity_repo.clone(),
        alert_repo,
        tag_repo,
        task_link_repo,
        sync_repo,
        config_repo: config_repo.clone(),
        worklog_repo: worklog_repo.clone(),
        recurrence_repo,
        gryzzly_catalog_repo,
        timesheet_draft_repo,
        signal_mapping_repo,
        memory_repo,
        memory_retriever,
        memory_file_source,
        git_connector,
        graph_token_provider: graph_token_provider.clone(),
        session_repo: session_repo.clone(),
    };
    let schema = graphql::schema::build_schema(deps);

    if let Some(Command::ExportSchema) = cli.command {
        println!("{}", schema.sdl());
        return;
    }

    // Migration 014 leaves `activity_slots.source` NULL. Classify those rows once,
    // from the data itself, before anything can rebuild a half-day: a NULL is read
    // as `Manual`, so an unclassified flush-derived slot would survive a rebuild and
    // the same morning would be counted twice. Deliberately after the `ExportSchema`
    // early return above: that path is `cargo run -p api -- export-schema`, the
    // documented codegen command run against the real `DATABASE_URL` with its
    // stdout redirected into `schema.graphql` — a codegen command must not also
    // perform a one-shot irreversible write, and `tracing_subscriber::fmt()`'s
    // default writer is stdout, so its own `tracing::info!` would land in the same
    // file as the SDL. No request can reach the router below until `axum::serve`
    // starts several lines down, so running the pass here costs nothing either way.
    match application::use_cases::slot_classification::classify_slot_sources(
        activity_repo.as_ref(),
        worklog_repo.as_ref(),
        config_repo.as_ref(),
        default_user_id,
        chrono::NaiveDate::from_ymd_opt(2020, 1, 1).expect("static date"),
        chrono::Utc::now().date_naive(),
        chrono::Utc::now(),
    )
    .await
    {
        Ok(outcome) if outcome.skipped => {
            tracing::debug!("slot provenance already classified");
        }
        Ok(outcome) => tracing::info!(
            worklog = outcome.worklog,
            manual = outcome.manual,
            "classified pre-014 activity slot provenance"
        ),
        // A failure here must not stop the server: every unclassified row reads as
        // `Manual`, which is the conservative value, and the pass retries on the
        // next boot because the guard key was never written.
        Err(e) => tracing::error!("slot provenance classification failed: {e}"),
    }

    let mut app = Router::new()
        .route("/graphql", post(graphql::schema::graphql_handler))
        .route("/auth/microsoft/login", get(auth::microsoft::login))
        .route("/auth/microsoft/callback", get(auth::microsoft::callback));
    if cfg!(debug_assertions) {
        app = app.route("/graphql/playground", get(graphql::schema::graphql_playground));
    }
    let app = app
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

    tokio::spawn(jobs::run_eod_scheduler(eod_deps, default_user_id));

    let session_reaper_deps = jobs::SessionReaperDeps {
        session_repo,
        worklog_repo: worklog_repo.clone(),
        activity_repo: activity_repo.clone(),
        config_repo: config_repo.clone(),
    };
    tokio::spawn(jobs::run_session_reaper_scheduler(session_reaper_deps, default_user_id));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    tracing::info!("Server running on http://{}", addr);
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
