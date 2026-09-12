use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

mod auth;
mod graphql;
mod jobs;
mod security;
mod state;

use uuid::Uuid;

use application::repositories::{BreakEventRepository, BreakRuleRepository, ClaudeUsageRepository};
use application::services::{NullNotifier, NullSurface, Notifier, SurfaceController};
use graphql::schema::SchemaDeps;
use infrastructure::database::*;
use infrastructure::connectors::claude_transcripts::FsClaudeTranscriptSource;
use infrastructure::connectors::microsoft::oauth::{MicrosoftOAuth, MicrosoftOAuthConfig};
use infrastructure::connectors::microsoft::token_provider::RefreshingGraphTokenProvider;
use infrastructure::notify::{HudToggleSurface, NotifySendNotifier};

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

/// Builds the full HTTP router: `/graphql` (CSRF-guarded, see `security.rs`),
/// the OAuth redirect routes, the debug GraphiQL playground (debug builds
/// only), CORS, and tracing. Extracted out of `main` so tests can drive the
/// router `main` actually serves instead of a hand-rolled stand-in that could
/// silently drift from it -- see the `require_csrf_header` regression this
/// guards against.
pub(crate) fn build_router(state: state::AppState, static_dir: Option<PathBuf>) -> Router {
    let mut app = Router::new()
        .route(
            "/graphql",
            // Scoped to this one route via `MethodRouter::layer`, not
            // `Router::layer` -- the OAuth `GET` redirects below and the debug
            // playground route must stay reachable without this header. See
            // `security.rs` for what this defends against and why.
            post(graphql::schema::graphql_handler)
                .layer(axum::middleware::from_fn(security::require_csrf_header)),
        )
        .route("/auth/microsoft/login", get(auth::microsoft::login))
        .route("/auth/microsoft/callback", get(auth::microsoft::callback));
    if cfg!(debug_assertions) {
        app = app.route("/graphql/playground", get(graphql::schema::graphql_playground));
    }
    let app = app.layer(
        CorsLayer::new()
            .allow_origin([
                // The Vite dev server, used by `pnpm dev` and by the Tauri HUD's
                // own `devUrl` during `tauri dev`.
                "http://localhost:3000".parse::<axum::http::HeaderValue>().unwrap(),
                // The aplan HUD (Tauri v2 desktop overlay, see
                // docs/plans/2026-08-27-hud-overlay-plan-1-coque-tauri.md): its
                // production window loads the bundled frontend through Tauri's
                // custom asset protocol, whose origin on Linux is `tauri://localhost`
                // -- not `http://localhost:3000`, so every GraphQL request from the
                // built app was previously rejected by this layer. Confirmed
                // empirically (not guessed): a throwaway `tauri::WebviewWindowBuilder`
                // probe built with the same `custom-protocol` feature flag Tauri's
                // own CLI passes for production builds, loaded via `WebviewUrl::App`
                // exactly like the real HUD, was pointed at a local echo server and
                // its raw request logged `Origin: tauri://localhost` on the wire (not
                // just what devtools would display, which has been known to diverge
                // from the header WebKitGTK actually sends -- see
                // https://github.com/tauri-apps/wry/issues/366). This origin is not
                // app-specific -- every Tauri v2 app on Linux/macOS shares
                // `tauri://localhost` -- and CORS is browser-enforced, so it never
                // gated local non-browser processes anyway (`curl` against this
                // endpoint always worked and still does). This entry doesn't
                // meaningfully widen exposure because `graphql_handler` doesn't
                // authenticate per request at all (it resolves everything against a
                // hardcoded `default_user_id`): reachability already equals authority
                // here. Do not delete this entry -- the HUD needs it, and CORS is
                // currently the only thing standing between a visited website and the
                // cockpit. Windows uses `http://tauri.localhost` instead (a third
                // entry, not added -- bundling is disabled today so this is Linux-only).
                "tauri://localhost".parse::<axum::http::HeaderValue>().unwrap(),
            ])
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
            // `x-aplan-client` must be preflight-approved for the real
            // frontend's cross-origin requests to succeed at all -- see
            // `security.rs` for why this header exists.
            .allow_headers([
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderName::from_static(security::CSRF_HEADER_NAME),
            ]),
    )
    .layer(TraceLayer::new_for_http());
    // Le service statique est monté en `fallback_service` : il ne voit que ce
    // qu'aucune route n'a résolu, donc il ne peut ni masquer `/graphql` ni
    // court-circuiter `require_csrf_header`. Absent, le routeur est
    // rigoureusement celui d'avant -- c'est ce que garantit
    // `unknown_route_is_404_without_static_dir`.
    let app = match static_dir {
        Some(dir) => {
            // `.fallback(...)`, pas `.not_found_service(...)` : ce dernier force
            // le statut de la réponse à 404 (`SetStatus`, voir tower-http
            // `ServeDir::not_found_service`), ce qui casserait le contrat de
            // `serves_index_for_unknown_route_when_static_dir_set` (200 attendu).
            // `.fallback` invoque `index.html` sans toucher au statut que
            // `ServeFile` renvoie lui-même -- 200, puisque le fichier existe.
            let index = ServeFile::new(dir.join("index.html"));
            app.fallback_service(ServeDir::new(dir).fallback(index))
        }
        None => app,
    };
    app.with_state(state)
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
    // Cloned here for the same reason as `recurrence_repo_for_jobs` below: both are
    // moved into the schema builder further down.
    let task_repo_for_jobs = task_repo.clone();
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
    // Cloned here, not below: `recurrence_repo` is *moved* into the schema builder
    // further down, so a clone written after that line would not compile. Same
    // manoeuvre the break scheduler already documents.
    let recurrence_repo_for_jobs = recurrence_repo.clone();
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

    // Built once in main.rs and handed to both consumers: the background job below and
    // the GraphQL `SchemaDeps` of Tasks 8-9, so there is one instance, not two.
    let break_rule_repo: Arc<dyn BreakRuleRepository> =
        Arc::new(SqliteBreakRuleRepository::new(db_pool.clone()));
    let break_event_repo: Arc<dyn BreakEventRepository> =
        Arc::new(SqliteBreakEventRepository::new(db_pool.clone()));
    // Same reason: read by the GraphQL resolver and written by the indexing job.
    let claude_usage_repo: Arc<dyn ClaudeUsageRepository> =
        Arc::new(SqliteClaudeUsageRepository::new(db_pool.clone()));

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
        activity_repo: activity_repo.clone(),
    };

    let deps = SchemaDeps {
        task_repo,
        // Cloned, not moved: the break scheduler spawned below also needs it.
        meeting_repo: meeting_repo.clone(),
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
        break_rule_repo: break_rule_repo.clone(),
        break_event_repo: break_event_repo.clone(),
        claude_usage_repo: claude_usage_repo.clone(),
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

    // Répertoire du frontend compilé. Non configuré => l'API se comporte
    // exactement comme avant : le service statique n'existe que pour le tunnel.
    let static_dir = std::env::var("APLAN_STATIC_DIR").ok().map(PathBuf::from);
    let app = build_router(
        state::AppState {
            schema: schema.clone(),
            config_repo: config_repo.clone(),
            oauth: oauth.clone(),
            default_user_id,
            oauth_state: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        },
        static_dir,
    );

    tokio::spawn(jobs::run_eod_scheduler(eod_deps, default_user_id));

    // The notifier is chosen once, at startup: a headless run keeps its books silently
    // rather than failing every 30 seconds on a bus that is not there.
    let notifier: Arc<dyn Notifier> = if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok() {
        Arc::new(NotifySendNotifier::new())
    } else {
        tracing::info!("no session bus: break notifications will be recorded, not shown");
        Arc::new(NullNotifier)
    };
    // Same criterion, same reason: without a session bus there is no compositor to
    // raise the overlay on. The break still runs and is still recorded — the backend
    // owns the countdown — it simply has nothing to show.
    let surface: Arc<dyn SurfaceController> = if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok() {
        Arc::new(HudToggleSurface::new())
    } else {
        tracing::info!("no session bus: breaks will run without their overlay");
        Arc::new(NullSurface)
    };
    tokio::spawn(jobs::run_break_scheduler(
        jobs::BreakDeps {
            rule_repo: break_rule_repo.clone(),
            event_repo: break_event_repo.clone(),
            meeting_repo: meeting_repo.clone(),
            config_repo: config_repo.clone(),
            notifier,
            surface,
        },
        default_user_id,
    ));

    let session_reaper_deps = jobs::SessionReaperDeps {
        session_repo,
        worklog_repo: worklog_repo.clone(),
        activity_repo: activity_repo.clone(),
        config_repo: config_repo.clone(),
    };
    tokio::spawn(jobs::run_session_reaper_scheduler(session_reaper_deps, default_user_id));

    // Only when there is a transcript tree to read. No HOME (a container, a system
    // unit with a scrubbed environment) means no Claude Code on this machine, and a
    // job looping over a path that cannot exist is noise, not resilience.
    match FsClaudeTranscriptSource::from_home() {
        Some(source) => {
            tokio::spawn(jobs::run_claude_usage_scheduler(jobs::ClaudeUsageDeps {
                source: Arc::new(source),
                repo: claude_usage_repo.clone(),
            }));
        }
        None => tracing::info!("no HOME: the Claude usage index will not be built"),
    }

    tokio::spawn(jobs::run_recurrence_scheduler(
        jobs::RecurrenceDeps {
            rec_repo: recurrence_repo_for_jobs,
            task_repo: task_repo_for_jobs,
            worklog_repo: worklog_repo.clone(),
        },
        default_user_id,
    ));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    tracing::info!("Server running on http://{}", addr);
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Extrait ici plutôt que dupliqué dans `security.rs` : les deux modules de
// tests (celui-ci et `security::tests`) ont besoin du même `AppState` réel
// pour driver `build_router`, et un `pub(crate)` sur ce module est plus
// propre qu'un aller-retour d'imports entre les deux fichiers.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    use infrastructure::connectors::git::ShellGitConnector;
    use infrastructure::connectors::memory_files::FsMemoryFileSource;

    /// Builds the exact `AppState` `main` builds, backed by an in-memory,
    /// migrated, seeded SQLite DB (see `create_sqlite_pool`) instead of a
    /// hand-rolled stand-in. This is what lets the tests below (and
    /// `security::tests`) drive `crate::build_router` -- the router `main`
    /// actually serves -- rather than a look-alike that could silently drift
    /// from it.
    pub(crate) async fn test_app_state() -> state::AppState {
        let pool = create_sqlite_pool("sqlite::memory:")
            .await
            .expect("in-memory sqlite pool");

        let config_repo: Arc<dyn application::repositories::ConfigRepository> =
            Arc::new(SqliteConfigRepository::new(pool.clone()));
        let oauth = Arc::new(MicrosoftOAuth::new(MicrosoftOAuthConfig {
            client_id: String::new(),
            tenant_id: String::new(),
            client_secret: String::new(),
            redirect_uri: "http://localhost:3001/auth/microsoft/callback".to_string(),
        }));
        let graph_token_provider: Arc<dyn application::services::GraphTokenProvider> =
            Arc::new(RefreshingGraphTokenProvider::new(config_repo.clone(), oauth.clone()));

        let deps = SchemaDeps {
            task_repo: Arc::new(SqliteTaskRepository::new(pool.clone())),
            meeting_repo: Arc::new(SqliteMeetingRepository::new(pool.clone())),
            project_repo: Arc::new(SqliteProjectRepository::new(pool.clone())),
            activity_repo: Arc::new(SqliteActivitySlotRepository::new(pool.clone())),
            alert_repo: Arc::new(SqliteAlertRepository::new(pool.clone())),
            tag_repo: Arc::new(SqliteTagRepository::new(pool.clone())),
            task_link_repo: Arc::new(SqliteTaskLinkRepository::new(pool.clone())),
            sync_repo: Arc::new(SqliteSyncStatusRepository::new(pool.clone())),
            config_repo: config_repo.clone(),
            worklog_repo: Arc::new(SqliteWorklogRepository::new(pool.clone())),
            recurrence_repo: Arc::new(SqliteRecurrenceRepository::new(pool.clone())),
            gryzzly_catalog_repo: Arc::new(SqliteGryzzlyCatalogRepository::new(pool.clone())),
            timesheet_draft_repo: Arc::new(SqliteTimesheetDraftRepository::new(pool.clone())),
            signal_mapping_repo: Arc::new(SqliteSignalMappingRepository::new(pool.clone())),
            memory_repo: Arc::new(SqliteMemoryRepository::new(pool.clone())),
            memory_retriever: Arc::new(SqliteMemoryRetriever::new(pool.clone())),
            memory_file_source: Arc::new(FsMemoryFileSource::new()),
            git_connector: Arc::new(ShellGitConnector::new()),
            graph_token_provider,
            session_repo: Arc::new(SqliteSessionRepository::new(pool.clone())),
            break_rule_repo: Arc::new(SqliteBreakRuleRepository::new(pool.clone())),
            break_event_repo: Arc::new(SqliteBreakEventRepository::new(pool.clone())),
            claude_usage_repo: Arc::new(SqliteClaudeUsageRepository::new(pool.clone())),
        };

        state::AppState {
            schema: graphql::schema::build_schema(deps),
            config_repo,
            oauth,
            default_user_id: Uuid::parse_str(state::DEFAULT_USER_ID_STR).unwrap(),
            oauth_state: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request as HttpRequest, StatusCode};
    use tower::ServiceExt;

    use crate::test_support::test_app_state;

    #[tokio::test]
    async fn serves_index_for_unknown_route_when_static_dir_set() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<!doctype html>APP").unwrap();
        let req = HttpRequest::get("/m/new").body(Body::empty()).unwrap();
        let res = build_router(test_app_state().await, Some(dir.path().to_path_buf()))
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        // Le fallback SPA doit rendre index.html, pas un 404 : sans ça un
        // rechargement sur /m/new casse la PWA.
        assert_eq!(body, b"<!doctype html>APP".as_ref());
    }

    #[tokio::test]
    async fn graphql_still_requires_csrf_header_with_static_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "APP").unwrap();
        let req = HttpRequest::post("/graphql")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"query":"{ __typename }"}"#))
            .unwrap();
        let res = build_router(test_app_state().await, Some(dir.path().to_path_buf()))
            .oneshot(req)
            .await
            .unwrap();
        // Le service statique ne doit jamais avaler /graphql ni court-circuiter
        // le garde CSRF.
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn unknown_route_is_404_without_static_dir() {
        let req = HttpRequest::get("/m").body(Body::empty()).unwrap();
        let res = build_router(test_app_state().await, None)
            .oneshot(req)
            .await
            .unwrap();
        // Sans APLAN_STATIC_DIR le routeur reste rigoureusement l'actuel.
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
