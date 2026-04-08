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
    response_derives = "Debug, Clone, serde::Serialize"
)]
pub struct Health;
