use async_graphql::Object;

/// Root query type for the GraphQL schema.
#[derive(Default)]
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Health check query. Returns true if the server is running.
    async fn health(&self) -> bool {
        true
    }
}
