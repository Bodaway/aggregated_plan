use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};

/// Insert a freshly-issued state token, evicting any entries older than 10 minutes.
pub fn remember_state(store: &Mutex<HashMap<String, DateTime<Utc>>>, state: String, now: DateTime<Utc>) {
    let mut guard = store.lock().unwrap_or_else(|p| p.into_inner());
    guard.retain(|_, issued| now - *issued < Duration::minutes(10));
    guard.insert(state, now);
}

/// Validate + consume a state token. Returns true if present and younger than 10 minutes.
pub fn consume_state(store: &Mutex<HashMap<String, DateTime<Utc>>>, state: &str, now: DateTime<Utc>) -> bool {
    let mut guard = store.lock().unwrap_or_else(|p| p.into_inner());
    match guard.remove(state) {
        Some(issued) => now - issued < Duration::minutes(10),
        None => false,
    }
}

use axum::extract::{Query, State};
use axum::response::Redirect;
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

const SPA_ROOT: &str = "http://localhost:3000";

/// GET /auth/microsoft/login — redirect the browser to Microsoft's authorize page.
pub async fn login(State(state): State<AppState>) -> Redirect {
    let csrf = Uuid::new_v4().to_string();
    remember_state(&state.oauth_state, csrf.clone(), Utc::now());
    Redirect::temporary(&state.oauth.authorize_url(&csrf))
}

#[derive(Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// GET /auth/microsoft/callback — exchange the code and persist tokens.
pub async fn callback(
    State(app): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Redirect {
    if let Some(err) = params.error {
        let reason = params.error_description.unwrap_or(err);
        return Redirect::temporary(&format!(
            "{SPA_ROOT}/?auth=error&reason={}",
            urlencoding::encode(&reason)
        ));
    }
    let (Some(code), Some(state)) = (params.code, params.state) else {
        return Redirect::temporary(&format!(
            "{SPA_ROOT}/?auth=error&reason=missing_code"
        ));
    };
    if !consume_state(&app.oauth_state, &state, Utc::now()) {
        return Redirect::temporary(&format!(
            "{SPA_ROOT}/?auth=error&reason=bad_state"
        ));
    }
    let tokens = match app.oauth.exchange_code(&code).await {
        Ok(t) => t,
        Err(e) => {
            return Redirect::temporary(&format!(
                "{SPA_ROOT}/?auth=error&reason={}",
                urlencoding::encode(&e.to_string())
            ))
        }
    };
    let uid = app.default_user_id;
    let persist_err = || Redirect::temporary(&format!("{SPA_ROOT}/?auth=error&reason=persist_failed"));

    if app.config_repo.set(uid, "microsoft.access_token", &tokens.access_token).await.is_err() {
        return persist_err();
    }
    if app.config_repo.set(uid, "microsoft.token_expires_at", &tokens.expires_at.to_rfc3339()).await.is_err() {
        return persist_err();
    }
    if let Some(rt) = &tokens.refresh_token {
        if app.config_repo.set(uid, "microsoft.refresh_token", rt).await.is_err() {
            return persist_err();
        }
    }
    if let Some(acct) = &tokens.account {
        if app.config_repo.set(uid, "microsoft.account", acct).await.is_err() {
            return persist_err();
        }
    }
    Redirect::temporary(&format!("{SPA_ROOT}/?auth=connected"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_state_consumed_once() {
        let s = Mutex::new(HashMap::new());
        let now = Utc::now();
        remember_state(&s, "abc".into(), now);
        assert!(consume_state(&s, "abc", now));
        assert!(!consume_state(&s, "abc", now)); // single use
    }

    #[test]
    fn unknown_state_rejected() {
        let s = Mutex::new(HashMap::new());
        assert!(!consume_state(&s, "nope", Utc::now()));
    }

    #[test]
    fn expired_state_rejected() {
        let s = Mutex::new(HashMap::new());
        let issued = Utc::now() - Duration::minutes(11);
        remember_state(&s, "old".into(), issued);
        assert!(!consume_state(&s, "old", Utc::now()));
    }
}
