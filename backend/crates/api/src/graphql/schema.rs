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

/// Build the async-graphql schema with all repository instances injected as data.
pub fn build_schema(
    task_repo: Arc<dyn TaskRepository>,
    meeting_repo: Arc<dyn MeetingRepository>,
    project_repo: Arc<dyn ProjectRepository>,
    activity_repo: Arc<dyn ActivitySlotRepository>,
    alert_repo: Arc<dyn AlertRepository>,
    tag_repo: Arc<dyn TagRepository>,
    task_link_repo: Arc<dyn TaskLinkRepository>,
    sync_repo: Arc<dyn SyncStatusRepository>,
    config_repo: Arc<dyn ConfigRepository>,
) -> AppSchema {
    // Default user for local development
    let default_user_id: UserId =
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("valid default UUID");

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
