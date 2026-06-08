use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

/// Static, app-level OAuth configuration (sourced from environment, never the DB).
#[derive(Clone)]
pub struct OutlookOAuthConfig {
    pub client_id: String,
    pub tenant_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl OutlookOAuthConfig {
    pub fn authorize_endpoint(&self) -> String {
        format!("https://login.microsoftonline.com/{}/oauth2/v2.0/authorize", self.tenant_id)
    }
    pub fn token_endpoint(&self) -> String {
        format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", self.tenant_id)
    }
    pub fn scope(&self) -> &'static str {
        "https://graph.microsoft.com/Calendars.Read offline_access openid profile"
    }
}

/// A normalized token result from either a code exchange or a refresh.
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub account: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    pub id_token: Option<String>,
}

/// Tokens should be refreshed if they are within 60s of expiry (or already expired).
pub fn should_refresh(now: DateTime<Utc>, expires_at: DateTime<Utc>) -> bool {
    now + Duration::seconds(60) >= expires_at
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn refreshes_when_expired() {
        let now = Utc::now();
        assert!(should_refresh(now, now - Duration::seconds(1)));
    }

    #[test]
    fn refreshes_within_skew_window() {
        let now = Utc::now();
        assert!(should_refresh(now, now + Duration::seconds(30)));
    }

    #[test]
    fn does_not_refresh_when_fresh() {
        let now = Utc::now();
        assert!(!should_refresh(now, now + Duration::seconds(600)));
    }
}
