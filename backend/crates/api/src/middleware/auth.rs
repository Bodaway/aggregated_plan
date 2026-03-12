use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

use crate::context::UserContext;

/// Default user ID for local development mode.
const DEFAULT_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

/// Auth middleware that injects a default UserContext for local development.
/// In production, this would extract and validate a JWT or session token.
pub async fn auth_middleware(mut req: Request, next: Next) -> Response {
    let user_id = Uuid::parse_str(DEFAULT_USER_ID).expect("default user ID is valid");
    let ctx = UserContext { user_id };
    req.extensions_mut().insert(ctx);
    next.run(req).await
}
