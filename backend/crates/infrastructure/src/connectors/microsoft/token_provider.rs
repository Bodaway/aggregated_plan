use std::sync::Arc;

use application::errors::{AppError, ConnectorError};
use application::repositories::ConfigRepository;
use application::services::GraphTokenProvider;
use async_trait::async_trait;
use chrono::Utc;
use domain::types::{Source, UserId};

use super::oauth::{should_refresh, MicrosoftOAuth};

pub struct RefreshingGraphTokenProvider {
    config_repo: Arc<dyn ConfigRepository>,
    oauth: Arc<MicrosoftOAuth>,
}

impl RefreshingGraphTokenProvider {
    pub fn new(config_repo: Arc<dyn ConfigRepository>, oauth: Arc<MicrosoftOAuth>) -> Self {
        Self { config_repo, oauth }
    }
}

#[async_trait]
impl GraphTokenProvider for RefreshingGraphTokenProvider {
    async fn valid_access_token(&self, user_id: UserId) -> Result<String, AppError> {
        let access = self.config_repo.get(user_id, "microsoft.access_token").await?;
        let expires = self.config_repo.get(user_id, "microsoft.token_expires_at").await?;
        if let (Some(a), Some(e)) = (&access, &expires) {
            if !a.is_empty() {
                if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(e) {
                    if !should_refresh(Utc::now(), exp.with_timezone(&Utc)) {
                        return Ok(a.clone());
                    }
                }
            }
        }

        let refresh_token = self
            .config_repo
            .get(user_id, "microsoft.refresh_token")
            .await?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Connector {
                connector_source: Source::Outlook,
                message: "Sign-in required".to_string(),
            })?;

        let tokens = match self.oauth.refresh(&refresh_token).await {
            Ok(t) => t,
            Err(e) => {
                // On a definitive invalid_grant (HTTP 400 with invalid_grant body):
                // clear stored tokens so the auth gate will show "sign in".
                // Transient errors (network, 5xx) surface as non-AuthFailed variants
                // and do NOT clear the keys.
                if matches!(e, ConnectorError::AuthFailed { .. }) {
                    let _ = self.config_repo.set(user_id, "microsoft.refresh_token", "").await;
                    let _ = self.config_repo.set(user_id, "microsoft.access_token", "").await;
                }
                return Err(AppError::Connector {
                    connector_source: Source::Outlook,
                    message: format!("Sign-in required: {e}"),
                });
            }
        };

        self.config_repo
            .set(user_id, "microsoft.access_token", &tokens.access_token)
            .await?;
        self.config_repo
            .set(
                user_id,
                "microsoft.token_expires_at",
                &tokens.expires_at.to_rfc3339(),
            )
            .await?;
        if let Some(rt) = &tokens.refresh_token {
            self.config_repo
                .set(user_id, "microsoft.refresh_token", rt)
                .await?;
        }
        Ok(tokens.access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use application::errors::RepositoryError;

    struct FakeConfig(Mutex<HashMap<String, String>>);

    #[async_trait]
    impl ConfigRepository for FakeConfig {
        async fn get(&self, _u: UserId, k: &str) -> Result<Option<String>, RepositoryError> {
            Ok(self.0.lock().unwrap().get(k).cloned())
        }
        async fn get_all(&self, _u: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
        async fn set(&self, _u: UserId, k: &str, v: &str) -> Result<(), RepositoryError> {
            self.0.lock().unwrap().insert(k.to_string(), v.to_string());
            Ok(())
        }
    }

    fn uid() -> UserId {
        uuid::Uuid::nil()
    }

    #[tokio::test]
    async fn returns_cached_token_when_fresh() {
        let mut m = HashMap::new();
        m.insert("microsoft.access_token".into(), "cached-abc".into());
        m.insert(
            "microsoft.token_expires_at".into(),
            (Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
        );
        let cfg = Arc::new(FakeConfig(Mutex::new(m)));
        let oauth = Arc::new(MicrosoftOAuth::new(super::super::oauth::MicrosoftOAuthConfig {
            client_id: "c".into(),
            tenant_id: "t".into(),
            client_secret: "s".into(),
            redirect_uri: "http://localhost:3001/auth/microsoft/callback".into(),
        }));
        let provider = RefreshingGraphTokenProvider::new(cfg, oauth);
        assert_eq!(
            provider.valid_access_token(uid()).await.unwrap(),
            "cached-abc"
        );
    }

    #[tokio::test]
    async fn errors_sign_in_required_when_no_refresh_token() {
        let cfg = Arc::new(FakeConfig(Mutex::new(HashMap::new())));
        let oauth = Arc::new(MicrosoftOAuth::new(super::super::oauth::MicrosoftOAuthConfig {
            client_id: "c".into(),
            tenant_id: "t".into(),
            client_secret: "s".into(),
            redirect_uri: "http://localhost:3001/auth/microsoft/callback".into(),
        }));
        let provider = RefreshingGraphTokenProvider::new(cfg, oauth);
        let err = provider.valid_access_token(uid()).await.unwrap_err();
        assert!(err.to_string().contains("Sign-in required"));
    }
}
