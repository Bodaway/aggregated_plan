//! Compile-time-checked GraphQL operations. Each `GraphQLQuery` derive references
//! a file under `graphql/` and validates it against `graphql/schema.graphql` at
//! build time. Adding a new operation is two steps: write the .graphql file,
//! add a derive here.

use graphql_client::GraphQLQuery;

// Custom scalar mappings used by the codegen.
#[allow(non_camel_case_types)]
type DateTime = String;
#[allow(non_camel_case_types)]
type NaiveDate = String;
#[allow(non_camel_case_types)]
type ID = String;
#[allow(non_camel_case_types)]
type JSON = serde_json::Value;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/health.graphql",
    response_derives = "Debug, Clone"
)]
pub struct Health;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/find_task_by_source_id.graphql",
    response_derives = "Debug, Clone"
)]
pub struct FindTaskBySourceId;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/find_tasks_by_title.graphql",
    response_derives = "Debug, Clone"
)]
pub struct FindTasksByTitle;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/current_activity.graphql",
    response_derives = "Debug, Clone"
)]
pub struct CurrentActivity;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/start_activity.graphql",
    response_derives = "Debug, Clone"
)]
pub struct StartActivity;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/stop_activity.graphql",
    response_derives = "Debug, Clone"
)]
pub struct StopActivity;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/append_task_notes.graphql",
    response_derives = "Debug, Clone"
)]
pub struct AppendTaskNotes;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/update_task_status.graphql",
    response_derives = "Debug, Clone"
)]
pub struct UpdateTaskStatus;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/set_tracking_state.graphql",
    response_derives = "Debug, Clone"
)]
pub struct SetTrackingState;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/complete_task.graphql",
    response_derives = "Debug, Clone"
)]
pub struct CompleteTask;
