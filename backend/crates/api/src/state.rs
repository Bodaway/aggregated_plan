use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use domain::types::UserId;

use crate::graphql::schema::AppSchema;
use application::repositories::ConfigRepository;
use infrastructure::connectors::outlook::oauth::OutlookOAuth;

#[derive(Clone)]
pub struct AppState {
    pub schema: AppSchema,
    pub config_repo: Arc<dyn ConfigRepository>,
    pub oauth: Arc<OutlookOAuth>,
    pub default_user_id: UserId,
    /// CSRF state store: state token -> issued-at.
    pub oauth_state: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
}
