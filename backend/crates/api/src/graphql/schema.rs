use std::sync::Arc;

use async_graphql::http::GraphiQLSource;
use async_graphql::{EmptySubscription, MergedObject, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use domain::types::UserId;
use uuid::Uuid;

use super::mutation::MutationRoot;
use super::query::QueryRoot;
use super::subscription::SubscriptionRoot;
use crate::state::AppState;

use application::repositories::*;

/// Combined query root that merges the base QueryRoot with future module roots.
#[derive(MergedObject, Default)]
pub struct CombinedQuery(pub QueryRoot);

/// Combined mutation root that merges the base MutationRoot with future module roots.
#[derive(MergedObject, Default)]
pub struct CombinedMutation(pub MutationRoot);

pub type AppSchema = Schema<CombinedQuery, CombinedMutation, SubscriptionRoot>;

pub struct SchemaDeps {
    pub task_repo: Arc<dyn TaskRepository>,
    pub meeting_repo: Arc<dyn MeetingRepository>,
    pub project_repo: Arc<dyn ProjectRepository>,
    pub activity_repo: Arc<dyn ActivitySlotRepository>,
    pub alert_repo: Arc<dyn AlertRepository>,
    pub tag_repo: Arc<dyn TagRepository>,
    pub task_link_repo: Arc<dyn TaskLinkRepository>,
    pub sync_repo: Arc<dyn SyncStatusRepository>,
    pub config_repo: Arc<dyn ConfigRepository>,
    pub worklog_repo: Arc<dyn WorklogRepository>,
    pub recurrence_repo: Arc<dyn RecurrenceRepository>,
    pub outlook_token_provider: Arc<dyn application::services::OutlookTokenProvider>,
}

/// Build the async-graphql schema with all repository instances injected as data.
pub fn build_schema(deps: SchemaDeps) -> AppSchema {
    let SchemaDeps {
        task_repo,
        meeting_repo,
        project_repo,
        activity_repo,
        alert_repo,
        tag_repo,
        task_link_repo,
        sync_repo,
        config_repo,
        worklog_repo,
        recurrence_repo,
        outlook_token_provider,
    } = deps;
    // Default user for local development
    let default_user_id: UserId =
        Uuid::parse_str(crate::state::DEFAULT_USER_ID_STR).expect("valid default UUID");

    Schema::build(
        CombinedQuery(QueryRoot),
        CombinedMutation(MutationRoot),
        EmptySubscription,
    )
    .data(task_repo)
    .data(meeting_repo)
    .data(project_repo)
    .data(activity_repo)
    .data(alert_repo)
    .data(tag_repo)
    .data(task_link_repo)
    .data(sync_repo)
    .data(config_repo)
    .data(worklog_repo)
    .data(recurrence_repo)
    .data(outlook_token_provider)
    .data(default_user_id)
    .finish()
}

/// Handler for POST /graphql requests.
pub async fn graphql_handler(
    State(state): State<AppState>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    state.schema.execute(req.into_inner()).await.into()
}

/// Handler that serves the GraphiQL playground UI.
pub async fn graphql_playground() -> impl IntoResponse {
    Html(
        GraphiQLSource::build()
            .endpoint("/graphql")
            .finish(),
    )
}
