//! Where the Gryzzly `Authorization` header comes from.
//!
//! Two sources, in the order `forceSync` prefers them: a value pasted into
//! `gryzzly.token` configuration, else the local browser cookie. The pasted
//! value exists as an escape hatch — the cookie route depends on Chromium's
//! on-disk layout and keyring encryption, and this keeps a broken browser
//! upgrade from taking the sync down with it.

use std::path::PathBuf;

use application::errors::ConnectorError;
use application::services::GryzzlyTokenSource;
use async_trait::async_trait;
use chrono::Utc;

use super::cookie_jar;

/// Turn anything a human might paste into a valid header value.
pub fn normalise_header(raw: &str) -> String {
    let mut t = raw.trim();
    if t.len() >= 14 && t[..14].eq_ignore_ascii_case("authorization:") {
        t = t[14..].trim();
    }
    if t.len() >= 5 && t[..5].eq_ignore_ascii_case("user ") {
        return t.to_string();
    }
    format!("User {t}")
}

/// A token pasted into configuration.
pub struct StaticTokenSource {
    header: String,
}

impl StaticTokenSource {
    pub fn new(raw: &str) -> Self {
        Self { header: normalise_header(raw) }
    }
}

#[async_trait]
impl GryzzlyTokenSource for StaticTokenSource {
    async fn header_value(&self) -> Result<String, ConnectorError> {
        Ok(self.header.clone())
    }
}

/// The session cookie in a local Chromium-family profile.
pub struct BrowserCookieTokenSource {
    pinned: Option<PathBuf>,
}

impl BrowserCookieTokenSource {
    pub fn new(pinned: Option<PathBuf>) -> Self {
        Self { pinned }
    }

    /// Whether a `remember_token` row exists at all — expiry deliberately not
    /// checked. An expired cookie must reach the caller as the dated "log in
    /// again" message, not be flattened into `Not configured`.
    pub async fn available(&self) -> bool {
        cookie_jar::find_remember_token(self.pinned.as_deref()).await.is_ok()
    }
}

#[async_trait]
impl GryzzlyTokenSource for BrowserCookieTokenSource {
    async fn header_value(&self) -> Result<String, ConnectorError> {
        let value = cookie_jar::token_value(self.pinned.as_deref(), Utc::now()).await?;
        Ok(format!("User {value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_token_gains_the_user_prefix() {
        assert_eq!(normalise_header("abc123"), "User abc123");
    }

    #[test]
    fn an_existing_user_prefix_is_kept_once() {
        assert_eq!(normalise_header("User abc123"), "User abc123");
    }

    /// The bookmarklet in the sibling time-tracker app yields `User <tok>`, but a
    /// developer copying from DevTools pastes the whole header line.
    #[test]
    fn a_pasted_header_line_is_unwrapped() {
        assert_eq!(normalise_header("Authorization: User abc123"), "User abc123");
    }

    #[test]
    fn a_lowercase_prefix_is_not_doubled() {
        assert_eq!(normalise_header("user abc123"), "user abc123");
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        assert_eq!(normalise_header("  abc123\n"), "User abc123");
    }

    #[tokio::test]
    async fn static_source_returns_the_normalised_header() {
        let s = StaticTokenSource::new("abc123");
        assert_eq!(s.header_value().await.unwrap(), "User abc123");
    }

    #[tokio::test]
    async fn browser_source_pinned_at_a_missing_file_reports_configuration() {
        let src = BrowserCookieTokenSource::new(Some(PathBuf::from("/nonexistent/Cookies")));
        let err = src.header_value().await.unwrap_err();
        assert!(matches!(err, ConnectorError::Configuration(_)), "got {err:?}");
        assert!(!src.available().await);
    }
}
