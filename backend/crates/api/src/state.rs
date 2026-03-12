use crate::graphql::schema::AppSchema;

#[derive(Clone)]
pub struct AppState {
    pub schema: AppSchema,
}
