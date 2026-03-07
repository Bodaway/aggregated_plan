use domain::types::UserId;

/// Represents the authenticated user context extracted from the request.
/// In local development mode, a default user ID is injected.
#[derive(Debug, Clone)]
pub struct UserContext {
    pub user_id: UserId,
}
