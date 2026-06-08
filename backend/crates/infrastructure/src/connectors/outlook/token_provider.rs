use std::sync::Arc;

use application::errors::AppError;
use application::repositories::ConfigRepository;
use application::services::OutlookTokenProvider;
use async_trait::async_trait;
use chrono::Utc;
use domain::types::UserId;

use super::oauth::{should_refresh, OutlookOAuth};

pub struct RefreshingOutlookTokenProvider {
    config_repo: Arc<dyn ConfigRepository>,
    oauth: Arc<OutlookOAuth>,
}

impl RefreshingOutlookTokenProvider {
    pub fn new(config_repo: Arc<dyn ConfigRepository>, oauth: Arc<OutlookOAuth>) -> Self {
        Self { config_repo, oauth }
    }
}

#[async_trait]
impl OutlookTokenProvider for RefreshingOutlookTokenProvider {
    async fn valid_access_token(&self, user_id: UserId) -> Result<String, AppError> {
        let access = self.config_repo.get(user_id, "outlook.access_token").await?;
        let expires = self.config_repo.get(user_id, "outlook.token_expires_at").await?;
        let needs_refresh = match (&access, &expires) {
            (Some(a), Some(e)) if !a.is_empty() => {
                match chrono::DateTime::parse_from_rfc3339(e) {
                    Ok(exp) => should_refresh(Utc::now(), exp.with_timezone(&Utc)),
                    Err(_) => true,
                }
            }
            _ => true,
        };
        if !needs_refresh {
            return Ok(access.unwrap());
        }

        let refresh_token = self
            .config_repo
            .get(user_id, "outlook.refresh_token")
            .await?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Connector {
                connector_source: domain::types::Source::Outlook,
                message: "Reconnect required".to_string(),
            })?;

        let tokens = self
            .oauth
            .refresh(&refresh_token)
            .await
            .map_err(|e| AppError::Connector {
                connector_source: domain::types::Source::Outlook,
                message: format!("Reconnect required: {e}"),
            })?;

        self.config_repo
            .set(user_id, "outlook.access_token", &tokens.access_token)
            .await?;
        self.config_repo
            .set(
                user_id,
                "outlook.token_expires_at",
                &tokens.expires_at.to_rfc3339(),
            )
            .await?;
        if let Some(rt) = &tokens.refresh_token {
            self.config_repo
                .set(user_id, "outlook.refresh_token", rt)
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
        m.insert("outlook.access_token".into(), "cached-abc".into());
        m.insert(
            "outlook.token_expires_at".into(),
            (Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
        );
        let cfg = Arc::new(FakeConfig(Mutex::new(m)));
        let oauth = Arc::new(OutlookOAuth::new(super::super::oauth::OutlookOAuthConfig {
            client_id: "c".into(),
            tenant_id: "t".into(),
            client_secret: "s".into(),
            redirect_uri: "http://localhost:3001/auth/outlook/callback".into(),
        }));
        let provider = RefreshingOutlookTokenProvider::new(cfg, oauth);
        assert_eq!(
            provider.valid_access_token(uid()).await.unwrap(),
            "cached-abc"
        );
    }

    #[tokio::test]
    async fn errors_reconnect_required_when_no_refresh_token() {
        let cfg = Arc::new(FakeConfig(Mutex::new(HashMap::new())));
        let oauth = Arc::new(OutlookOAuth::new(super::super::oauth::OutlookOAuthConfig {
            client_id: "c".into(),
            tenant_id: "t".into(),
            client_secret: "s".into(),
            redirect_uri: "http://localhost:3001/auth/outlook/callback".into(),
        }));
        let provider = RefreshingOutlookTokenProvider::new(cfg, oauth);
        let err = provider.valid_access_token(uid()).await.unwrap_err();
        assert!(err.to_string().contains("Reconnect required"));
    }
}
