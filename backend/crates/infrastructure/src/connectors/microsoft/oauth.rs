use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use application::errors::ConnectorError;
use base64::Engine;

/// Static, app-level OAuth configuration (sourced from environment, never the DB).
#[derive(Clone)]
pub struct MicrosoftOAuthConfig {
    pub client_id: String,
    pub tenant_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl MicrosoftOAuthConfig {
    pub fn authorize_endpoint(&self) -> String {
        format!("https://login.microsoftonline.com/{}/oauth2/v2.0/authorize", self.tenant_id)
    }
    pub fn token_endpoint(&self) -> String {
        format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", self.tenant_id)
    }
    pub fn scope(&self) -> &'static str {
        "https://graph.microsoft.com/Calendars.Read https://graph.microsoft.com/Files.Read.All offline_access openid profile"
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

pub struct MicrosoftOAuth {
    config: MicrosoftOAuthConfig,
    http: reqwest::Client,
}

impl MicrosoftOAuth {
    pub fn new(config: MicrosoftOAuthConfig) -> Self {
        Self { config, http: reqwest::Client::new() }
    }

    pub fn config(&self) -> &MicrosoftOAuthConfig {
        &self.config
    }

    pub fn authorize_url(&self, state: &str) -> String {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("client_id", &self.config.client_id)
            .append_pair("response_type", "code")
            .append_pair("response_mode", "query")
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("scope", self.config.scope())
            .append_pair("state", state)
            .finish();
        format!("{}?{}", self.config.authorize_endpoint(), query)
    }

    pub async fn exchange_code(&self, code: &str) -> Result<TokenSet, ConnectorError> {
        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("scope", self.config.scope()),
        ];
        self.post_token(&params).await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenSet, ConnectorError> {
        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", self.config.scope()),
        ];
        self.post_token(&params).await
    }

    async fn post_token(&self, params: &[(&str, &str)]) -> Result<TokenSet, ConnectorError> {
        let resp = self.http
            .post(self.config.token_endpoint())
            .form(params)
            .send()
            .await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::BAD_REQUEST {
            let body = resp.text().await.unwrap_or_default();
            // Distinguish a permanent invalid_grant from other 400 errors so the
            // token provider can clear the stored tokens.
            if body.contains("invalid_grant") {
                return Err(ConnectorError::AuthFailed {
                    service: format!("Microsoft: invalid_grant -- {body}"),
                });
            }
            return Err(ConnectorError::AuthFailed { service: format!("Microsoft: {body}") });
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let body = resp.text().await.unwrap_or_default();
            return Err(ConnectorError::AuthFailed { service: format!("Microsoft: {body}") });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ConnectorError::Http { status: status.as_u16(), message: body });
        }
        let tr: TokenResponse = resp.json().await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;
        let expires_at = Utc::now() + Duration::seconds(tr.expires_in);
        let account = tr.id_token.as_deref().and_then(decode_upn);
        Ok(TokenSet {
            access_token: tr.access_token,
            refresh_token: tr.refresh_token,
            expires_at,
            account,
        })
    }
}

/// Decode the `preferred_username`/`upn` claim from an id_token JWT (no signature check —
/// the token came directly from the token endpoint over TLS, used only for display).
fn decode_upn(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    v.get("preferred_username").or_else(|| v.get("upn"))
        .and_then(|x| x.as_str()).map(String::from)
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

    fn test_config() -> MicrosoftOAuthConfig {
        MicrosoftOAuthConfig {
            client_id: "cid".into(),
            tenant_id: "tid".into(),
            client_secret: "sec".into(),
            redirect_uri: "http://localhost:3001/auth/microsoft/callback".into(),
        }
    }

    #[test]
    fn authorize_url_contains_required_params() {
        let oauth = MicrosoftOAuth::new(test_config());
        let url = oauth.authorize_url("xyz-state");
        assert!(url.starts_with("https://login.microsoftonline.com/tid/oauth2/v2.0/authorize?"));
        assert!(url.contains("client_id=cid"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=xyz-state"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3001%2Fauth%2Fmicrosoft%2Fcallback"));
        assert!(url.contains("Calendars.Read"));
        assert!(url.contains("Files.Read.All"));
        assert!(url.contains("offline_access"));
    }
}
