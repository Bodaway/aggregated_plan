use async_graphql::{Object, ID};

use domain::types::Tag;

/// GraphQL wrapper for the domain Tag entity.
pub struct TagGql(pub Tag);

#[Object]
impl TagGql {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn name(&self) -> &str {
        &self.0.name
    }

    async fn color(&self) -> Option<&str> {
        self.0.color.as_deref()
    }
}
