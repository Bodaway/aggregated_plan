# Outlook OAuth Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the manually-pasted Outlook access token with an interactive "Connect Outlook" sign-in (authorization-code flow) plus automatic refresh-token renewal, so calendar sync keeps working without user intervention.

**Architecture:** A confidential authorization-code flow against the existing Entra app `12dd5cbd`. Two new Axum routes (`/auth/outlook/login`, `/auth/outlook/callback`) drive the browser sign-in and persist refresh+access tokens in the `configuration` table. A `RefreshingOutlookTokenProvider` (infrastructure) hands the sync path a always-fresh access token, refreshing via the refresh token when expired. The frontend Settings page swaps the token textbox for Connect/Disconnect buttons.

**Tech Stack:** Rust (Axum 0.7, async-graphql 7, sqlx, reqwest 0.12, async_trait, thiserror, chrono, serde), React 18 + urql + TypeScript, Microsoft Graph v1.0.

---

## Fixed parameters (from the design spec)

| Item | Value |
|------|-------|
| Tenant ID | `0ca0e5b0-fbba-4994-839d-8d47b96d86db` |
| Client ID | `12dd5cbd-f897-4184-a473-8effc7a93aba` |
| Authorize | `https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize` |
| Token | `https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token` |
| Scopes | `https://graph.microsoft.com/Calendars.Read offline_access openid profile` |
| Redirect URI | `http://localhost:3001/auth/outlook/callback` |
| Frontend Settings | `http://localhost:3000/settings` |

## File structure

- **Create** `backend/crates/application/src/services/outlook_token_provider.rs` — `OutlookTokenProvider` trait.
- **Create** `backend/crates/infrastructure/src/connectors/outlook/oauth.rs` — `OutlookOAuthConfig`, `OutlookOAuth` (authorize URL / code exchange / refresh), `TokenSet`, `should_refresh`.
- **Create** `backend/crates/infrastructure/src/connectors/outlook/token_provider.rs` — `RefreshingOutlookTokenProvider`.
- **Create** `backend/crates/api/src/auth/mod.rs` + `backend/crates/api/src/auth/outlook.rs` — login/callback handlers + CSRF state store.
- **Modify** `backend/crates/application/src/services/mod.rs`, `backend/crates/infrastructure/src/connectors/outlook/mod.rs`, `backend/crates/infrastructure/src/connectors/mod.rs` — module exports.
- **Modify** `backend/crates/api/src/state.rs` — extend `AppState`.
- **Modify** `backend/crates/api/src/main.rs` — load OAuth env, build provider+oauth, add routes.
- **Modify** `backend/crates/api/src/graphql/schema.rs` — inject `Arc<dyn OutlookTokenProvider>`.
- **Modify** `backend/crates/api/src/graphql/mutation.rs` — `force_sync` uses the provider; add `disconnect_outlook`.
- **Modify** `backend/crates/api/src/graphql/query.rs` — add `outlook_connection`.
- **Modify** `backend/crates/application/src/use_cases/sync.rs` — honor `outlook.calendar_days`.
- **Modify** `backend/.env.example` — Outlook OAuth keys.
- **Modify** `frontend/src/hooks/use-settings.ts` + `frontend/src/pages/SettingsPage.tsx` — Connect/Disconnect UI.
- **Modify** `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md` — document the flow (French).

---

## Task 0: Entra app config + environment (prerequisite)

**Files:**
- Modify: `backend/.env` (local, git-ignored), `backend/.env.example`

This task uses the Witivio token (`az account get-access-token --resource-type ms-graph --tenant 0ca0e5b0-...`). The app object id for `12dd5cbd` must be fetched first.

- [ ] **Step 1: Add Calendars.Read + offline_access + Web redirect URI to the app**

```bash
TENANT=0ca0e5b0-fbba-4994-839d-8d47b96d86db
AT=$(az account get-access-token --resource-type ms-graph --tenant $TENANT --query accessToken -o tsv)
OBJ=$(curl -s -H "Authorization: Bearer $AT" \
  "https://graph.microsoft.com/v1.0/applications?\$filter=appId%20eq%20'12dd5cbd-f897-4184-a473-8effc7a93aba'&\$select=id" \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['value'][0]['id'])")
# PATCH: keep existing Files.Read.All (df85f4d6) + User.Read (e1fe6dd8); add Calendars.Read (465a38f9) + offline_access (7427e0e9); add Web redirect.
curl -s -X PATCH "https://graph.microsoft.com/v1.0/applications/$OBJ" \
  -H "Authorization: Bearer $AT" -H "Content-Type: application/json" -d '{
    "web": { "redirectUris": ["http://localhost:3001/auth/outlook/callback"] },
    "requiredResourceAccess": [{
      "resourceAppId": "00000003-0000-0000-c000-000000000000",
      "resourceAccess": [
        {"id":"df85f4d6-205c-4ac5-a5ea-6bf408dba283","type":"Scope"},
        {"id":"e1fe6dd8-ba31-4d61-89e7-88639da4683d","type":"Scope"},
        {"id":"465a38f9-76ea-45b9-9f34-9e8b0d4b0b42","type":"Scope"},
        {"id":"7427e0e9-2fba-42fe-b0c0-848c9e6a8182","type":"Scope"}
      ]
    }]
  }'
echo "patched"
```

Expected: `patched` and a subsequent GET shows the four scopes + the redirect URI.

- [ ] **Step 2: Create a client secret**

```bash
curl -s -X POST "https://graph.microsoft.com/v1.0/applications/$OBJ/addPassword" \
  -H "Authorization: Bearer $AT" -H "Content-Type: application/json" \
  -d '{"passwordCredential":{"displayName":"aggregated-plan-local"}}' \
  | python3 -c "import sys,json;print('SECRET:', json.load(sys.stdin)['secretText'])"
```

Expected: prints `SECRET: <value>`. Copy the value once — it cannot be retrieved again.

- [ ] **Step 3: Write `.env` (local) and `.env.example`**

Append to `backend/.env` (create if missing; never commit):

```
OUTLOOK_CLIENT_ID=12dd5cbd-f897-4184-a473-8effc7a93aba
OUTLOOK_TENANT_ID=0ca0e5b0-fbba-4994-839d-8d47b96d86db
OUTLOOK_CLIENT_SECRET=<secret from step 2>
OUTLOOK_REDIRECT_URI=http://localhost:3001/auth/outlook/callback
```

Append the placeholder block to `backend/.env.example`:

```
# Outlook OAuth (authorization-code flow)
OUTLOOK_CLIENT_ID=
OUTLOOK_TENANT_ID=
OUTLOOK_CLIENT_SECRET=
OUTLOOK_REDIRECT_URI=http://localhost:3001/auth/outlook/callback
```

- [ ] **Step 4: Verify `.env` is git-ignored**

Run: `cd backend && git check-ignore .env && echo IGNORED`
Expected: `IGNORED` (the path prints). If not ignored, add `.env` to `backend/.gitignore` and commit that.

- [ ] **Step 5: Commit the example only**

```bash
git add backend/.env.example
git commit -m "chore(env): document Outlook OAuth env vars"
```

---

## Task 1: `should_refresh` + OAuth config/types (pure logic, infrastructure)

**Files:**
- Create: `backend/crates/infrastructure/src/connectors/outlook/oauth.rs`
- Modify: `backend/crates/infrastructure/src/connectors/outlook/mod.rs`

- [ ] **Step 1: Write the failing test**

Create `oauth.rs` with only the test module + the function signature stubbed to `todo!()`:

```rust
use chrono::{DateTime, Duration, Utc};

/// Tokens should be refreshed if they are within 60s of expiry (or already expired).
pub fn should_refresh(now: DateTime<Utc>, expires_at: DateTime<Utc>) -> bool {
    todo!()
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
```

Add to `connectors/outlook/mod.rs`: `pub mod oauth;`

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test -p infrastructure should_refresh`
Expected: FAIL — panics with `not yet implemented` (todo!).

- [ ] **Step 3: Implement `should_refresh`**

Replace the `todo!()` body:

```rust
pub fn should_refresh(now: DateTime<Utc>, expires_at: DateTime<Utc>) -> bool {
    now + Duration::seconds(60) >= expires_at
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd backend && cargo test -p infrastructure should_refresh`
Expected: PASS (3 tests).

- [ ] **Step 5: Add the OAuth config + TokenSet types**

Append to `oauth.rs` (above the test module):

```rust
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
```

- [ ] **Step 6: Verify it compiles**

Run: `cd backend && cargo check -p infrastructure`
Expected: compiles (warnings about unused fields are fine for now).

- [ ] **Step 7: Commit**

```bash
git add backend/crates/infrastructure/src/connectors/outlook/oauth.rs backend/crates/infrastructure/src/connectors/outlook/mod.rs
git commit -m "feat(outlook): add OAuth config types and should_refresh helper"
```

---

## Task 2: Authorize-URL builder + token endpoint calls (`OutlookOAuth`)

**Files:**
- Modify: `backend/crates/infrastructure/src/connectors/outlook/oauth.rs`

- [ ] **Step 1: Write the failing test for authorize_url**

Add to the test module in `oauth.rs`:

```rust
fn test_config() -> OutlookOAuthConfig {
    OutlookOAuthConfig {
        client_id: "cid".into(),
        tenant_id: "tid".into(),
        client_secret: "sec".into(),
        redirect_uri: "http://localhost:3001/auth/outlook/callback".into(),
    }
}

#[test]
fn authorize_url_contains_required_params() {
    let oauth = OutlookOAuth::new(test_config());
    let url = oauth.authorize_url("xyz-state");
    assert!(url.starts_with("https://login.microsoftonline.com/tid/oauth2/v2.0/authorize?"));
    assert!(url.contains("client_id=cid"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("state=xyz-state"));
    assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3001%2Fauth%2Foutlook%2Fcallback"));
    assert!(url.contains("Calendars.Read"));
    assert!(url.contains("offline_access"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test -p infrastructure authorize_url_contains`
Expected: FAIL — `OutlookOAuth` not found.

- [ ] **Step 3: Implement `OutlookOAuth::new` + `authorize_url`**

Add to `oauth.rs` (above tests). This uses `reqwest` (already a dependency of infrastructure) and `url`-style encoding via `reqwest::Url`:

```rust
use crate::errors_compat::map_reqwest; // see note: replace with inline mapping below if no such module

pub struct OutlookOAuth {
    config: OutlookOAuthConfig,
    http: reqwest::Client,
}

impl OutlookOAuth {
    pub fn new(config: OutlookOAuthConfig) -> Self {
        Self { config, http: reqwest::Client::new() }
    }

    pub fn config(&self) -> &OutlookOAuthConfig {
        &self.config
    }

    pub fn authorize_url(&self, state: &str) -> String {
        let mut url = reqwest::Url::parse(&self.config.authorize_endpoint())
            .expect("valid authorize endpoint");
        url.query_pairs_mut()
            .append_pair("client_id", &self.config.client_id)
            .append_pair("response_type", "code")
            .append_pair("response_mode", "query")
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("scope", self.config.scope())
            .append_pair("state", state);
        url.to_string()
    }
}
```

NOTE: delete the `use crate::errors_compat::map_reqwest;` line — it was illustrative. No such import is needed for `authorize_url`.

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test -p infrastructure authorize_url_contains`
Expected: PASS.

- [ ] **Step 5: Add `exchange_code` and `refresh` (network methods)**

Look at `backend/crates/infrastructure/src/connectors/outlook/client.rs` for the existing `ConnectorError` import path (`crate::...` / `application::errors::ConnectorError`). Mirror it. Add to `impl OutlookOAuth`:

```rust
use application::services::OutlookEvent; // remove if unused; only ConnectorError is needed
use application::errors::ConnectorError;
use chrono::Duration as ChronoDuration;
use base64::Engine;

impl OutlookOAuth {
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
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::BAD_REQUEST {
            let body = resp.text().await.unwrap_or_default();
            return Err(ConnectorError::AuthFailed { service: format!("Outlook: {body}") });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ConnectorError::Http { status: status.as_u16(), message: body });
        }
        let tr: TokenResponse = resp.json().await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;
        let expires_at = Utc::now() + ChronoDuration::seconds(tr.expires_in);
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
```

Remove the `use application::services::OutlookEvent;` line (illustrative — not needed). Confirm `ConnectorError` import path matches `client.rs`.

- [ ] **Step 6: Ensure `base64` dependency is available**

Run: `cd backend && rg '^base64' crates/infrastructure/Cargo.toml`
If absent: `cargo add base64 -p infrastructure`. Re-run.

- [ ] **Step 7: Verify compile**

Run: `cd backend && cargo check -p infrastructure`
Expected: compiles.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/infrastructure
git commit -m "feat(outlook): authorize URL builder, code exchange, and refresh"
```

---

## Task 3: `OutlookTokenProvider` trait (application)

**Files:**
- Create: `backend/crates/application/src/services/outlook_token_provider.rs`
- Modify: `backend/crates/application/src/services/mod.rs`

- [ ] **Step 1: Create the trait**

`outlook_token_provider.rs`:

```rust
use async_trait::async_trait;
use domain::types::UserId;

use crate::errors::AppError;

/// Provides a currently-valid Microsoft Graph access token, refreshing it transparently.
#[async_trait]
pub trait OutlookTokenProvider: Send + Sync {
    /// Return a valid access token for the user, refreshing via the stored refresh token
    /// if the cached access token is missing or near expiry.
    async fn valid_access_token(&self, user_id: UserId) -> Result<String, AppError>;
}
```

- [ ] **Step 2: Export it**

In `services/mod.rs` add:

```rust
pub mod outlook_token_provider;
pub use outlook_token_provider::*;
```

- [ ] **Step 3: Verify compile**

Run: `cd backend && cargo check -p application`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/application/src/services
git commit -m "feat(outlook): OutlookTokenProvider trait"
```

---

## Task 4: `RefreshingOutlookTokenProvider` (infrastructure)

**Files:**
- Create: `backend/crates/infrastructure/src/connectors/outlook/token_provider.rs`
- Modify: `backend/crates/infrastructure/src/connectors/outlook/mod.rs`

Config keys used: `outlook.access_token`, `outlook.refresh_token`, `outlook.token_expires_at` (RFC3339), `outlook.account`.

- [ ] **Step 1: Write the failing test (refresh decision + persistence) with a fake config repo**

`token_provider.rs`:

```rust
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

        let refresh_token = self.config_repo.get(user_id, "outlook.refresh_token").await?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Connector {
                connector_source: domain::types::Source::Outlook,
                message: "Reconnect required".to_string(),
            })?;

        let tokens = self.oauth.refresh(&refresh_token).await.map_err(|e| AppError::Connector {
            connector_source: domain::types::Source::Outlook,
            message: format!("Reconnect required: {e}"),
        })?;

        self.config_repo.set(user_id, "outlook.access_token", &tokens.access_token).await?;
        self.config_repo.set(user_id, "outlook.token_expires_at", &tokens.expires_at.to_rfc3339()).await?;
        if let Some(rt) = &tokens.refresh_token {
            self.config_repo.set(user_id, "outlook.refresh_token", rt).await?;
        }
        Ok(tokens.access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use application::repositories::RepositoryError;

    struct FakeConfig(Mutex<HashMap<String, String>>);
    #[async_trait]
    impl ConfigRepository for FakeConfig {
        async fn get(&self, _u: UserId, k: &str) -> Result<Option<String>, RepositoryError> {
            Ok(self.0.lock().unwrap().get(k).cloned())
        }
        async fn get_all(&self, _u: UserId) -> Result<Vec<(String, String)>, RepositoryError> {
            Ok(self.0.lock().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        }
        async fn set(&self, _u: UserId, k: &str, v: &str) -> Result<(), RepositoryError> {
            self.0.lock().unwrap().insert(k.to_string(), v.to_string());
            Ok(())
        }
    }

    fn uid() -> UserId { uuid::Uuid::nil() }

    #[tokio::test]
    async fn returns_cached_token_when_fresh() {
        let mut m = HashMap::new();
        m.insert("outlook.access_token".into(), "cached-abc".into());
        m.insert("outlook.token_expires_at".into(), (Utc::now() + chrono::Duration::hours(1)).to_rfc3339());
        let cfg = Arc::new(FakeConfig(Mutex::new(m)));
        let oauth = Arc::new(OutlookOAuth::new(super::super::oauth::OutlookOAuthConfig {
            client_id: "c".into(), tenant_id: "t".into(),
            client_secret: "s".into(), redirect_uri: "http://localhost:3001/auth/outlook/callback".into(),
        }));
        let provider = RefreshingOutlookTokenProvider::new(cfg, oauth);
        assert_eq!(provider.valid_access_token(uid()).await.unwrap(), "cached-abc");
    }

    #[tokio::test]
    async fn errors_reconnect_required_when_no_refresh_token() {
        let cfg = Arc::new(FakeConfig(Mutex::new(HashMap::new())));
        let oauth = Arc::new(OutlookOAuth::new(super::super::oauth::OutlookOAuthConfig {
            client_id: "c".into(), tenant_id: "t".into(),
            client_secret: "s".into(), redirect_uri: "http://localhost:3001/auth/outlook/callback".into(),
        }));
        let provider = RefreshingOutlookTokenProvider::new(cfg, oauth);
        let err = provider.valid_access_token(uid()).await.unwrap_err();
        assert!(err.to_string().contains("Reconnect required"));
    }
}
```

Add `pub mod token_provider;` to `connectors/outlook/mod.rs`.

- [ ] **Step 2: Confirm `ConfigRepository` trait method set matches the fake**

Run: `cd backend && rg "async fn" crates/application/src/repositories/config_repository.rs`
Expected: exactly `get`, `get_all`, `set`. If the trait has more methods, add matching stubs to `FakeConfig`. Confirm `AppError: From<RepositoryError>` exists (search `impl From<RepositoryError> for AppError` in `application/src/errors.rs`); the `?` on `config_repo.get(...).await?` relies on it. If absent, map errors explicitly with `.map_err(AppError::from)` — but it should exist since use_cases use `?` on repo calls.

- [ ] **Step 3: Run tests**

Run: `cd backend && cargo test -p infrastructure refreshing_outlook -- --include-ignored; cargo test -p infrastructure returns_cached_token errors_reconnect_required`
Expected: both tests PASS.

- [ ] **Step 4: Verify full infra build + tests**

Run: `cd backend && cargo test -p infrastructure`
Expected: existing 50 tests + the new ones PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/infrastructure/src/connectors/outlook
git commit -m "feat(outlook): refreshing token provider with lazy refresh + rotation"
```

---

## Task 5: Honor `outlook.calendar_days` in sync (adjacent fix)

**Files:**
- Modify: `backend/crates/application/src/use_cases/sync.rs` (the `Source::Outlook` arm in `sync_source`, ~line 657-665)

- [ ] **Step 1: Replace the hardcoded 30-day window**

Find:

```rust
        Source::Outlook => {
            if let Some(client) = outlook_client {
                let today = Utc::now().date_naive();
                let end = today + chrono::Duration::days(30);
                sync_outlook(client, meeting_repo, sync_repo, user_id, (today, end)).await?;
```

Replace with:

```rust
        Source::Outlook => {
            if let Some(client) = outlook_client {
                let today = Utc::now().date_naive();
                let days: i64 = config_repo
                    .get(user_id, "outlook.calendar_days")
                    .await?
                    .and_then(|v| v.trim().parse::<i64>().ok())
                    .filter(|d| *d > 0)
                    .unwrap_or(14);
                let end = today + chrono::Duration::days(days);
                sync_outlook(client, meeting_repo, sync_repo, user_id, (today, end)).await?;
```

Confirm `config_repo` is in scope in `sync_source` (it is destructured from `SyncContext`). If `sync_source` accesses fields via `ctx`, use `ctx.config_repo` accordingly.

- [ ] **Step 2: Verify compile + tests**

Run: `cd backend && cargo test -p application sync`
Expected: existing sync tests PASS (the change is config-driven with a 14 default).

- [ ] **Step 3: Commit**

```bash
git add backend/crates/application/src/use_cases/sync.rs
git commit -m "fix(sync): honor outlook.calendar_days instead of hardcoded 30"
```

---

## Task 6: Inject provider into schema; `force_sync` uses fresh token

**Files:**
- Modify: `backend/crates/api/src/graphql/schema.rs` (`SchemaDeps`, `build_schema`)
- Modify: `backend/crates/api/src/graphql/mutation.rs` (`force_sync` outlook client block)

- [ ] **Step 1: Add provider to `SchemaDeps` and inject as data**

In `schema.rs`, add to `SchemaDeps`:

```rust
    pub outlook_token_provider: Arc<dyn application::services::OutlookTokenProvider>,
```

Destructure it in `build_schema` and add `.data(outlook_token_provider)` to the builder chain (next to `.data(config_repo)`).

- [ ] **Step 2: Use the provider in `force_sync`**

In `mutation.rs`, replace the existing `outlook_client` construction block:

```rust
        let outlook_client: Option<Arc<dyn OutlookClient>> = {
            let token = config_repo.get(*user_id, "outlook.access_token").await.ok().flatten();
            match token {
                Some(tok) if !tok.is_empty() => Some(Arc::new(GraphOutlookClient::new(tok))),
                _ => None,
            }
        };
```

with:

```rust
        let outlook_token_provider = ctx.data::<Arc<dyn OutlookTokenProvider>>()?;
        let outlook_client: Option<Arc<dyn OutlookClient>> =
            match outlook_token_provider.valid_access_token(*user_id).await {
                Ok(token) => Some(Arc::new(GraphOutlookClient::new(token))),
                Err(_) => None, // not connected or reconnect required; sync_source records the error
            };
```

Add the needed import at the top of `mutation.rs` if missing: `use application::services::OutlookTokenProvider;`. Confirm `GraphOutlookClient` and `OutlookClient` are already imported (they are, used in the original block).

- [ ] **Step 3: Verify compile**

Run: `cd backend && cargo check -p api`
Expected: FAILS at `main.rs` (SchemaDeps now needs `outlook_token_provider`) — that's Task 7. The `mutation.rs`/`schema.rs` edits themselves must be type-correct; read the error to confirm it is only the missing field in `main.rs`.

- [ ] **Step 4: Commit (after Task 7 compiles)**

Defer the commit until Task 7 makes the crate build. (Logical commit boundary: schema + main wiring together.)

---

## Task 7: Backend OAuth routes + AppState wiring

**Files:**
- Create: `backend/crates/api/src/auth/mod.rs`, `backend/crates/api/src/auth/outlook.rs`
- Modify: `backend/crates/api/src/state.rs`, `backend/crates/api/src/main.rs`

- [ ] **Step 1: Extend `AppState`**

`state.rs`:

```rust
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
```

Confirm the `infrastructure::connectors::outlook::oauth` path is public end-to-end (`connectors/mod.rs` has `pub mod outlook;`, `outlook/mod.rs` has `pub mod oauth;`). Add `pub` as needed.

- [ ] **Step 2: Write the failing test for the CSRF state store**

`auth/outlook.rs` (start with the store helpers + tests):

```rust
use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};

/// Insert a freshly-issued state token.
pub fn remember_state(store: &Mutex<HashMap<String, DateTime<Utc>>>, state: String, now: DateTime<Utc>) {
    store.lock().unwrap().insert(state, now);
}

/// Validate + consume a state token. Returns true if present and younger than 10 minutes.
pub fn consume_state(store: &Mutex<HashMap<String, DateTime<Utc>>>, state: &str, now: DateTime<Utc>) -> bool {
    let mut guard = store.lock().unwrap();
    match guard.remove(state) {
        Some(issued) => now - issued < Duration::minutes(10),
        None => false,
    }
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
```

Create `auth/mod.rs`:

```rust
pub mod outlook;
```

Add `mod auth;` to `main.rs`.

- [ ] **Step 3: Run the store tests**

Run: `cd backend && cargo test -p api consume_state valid_state unknown_state expired_state`
Expected: PASS (3 tests). (This will only compile once `main.rs` has `mod auth;` and AppState compiles; if AppState references aren't ready, temporarily run `cargo test -p api --no-run` and fix paths.)

- [ ] **Step 4: Implement the login + callback handlers**

Append to `auth/outlook.rs`:

```rust
use axum::extract::{Query, State};
use axum::response::Redirect;
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

const SETTINGS_URL: &str = "http://localhost:3000/settings";

/// GET /auth/outlook/login — redirect the browser to Microsoft's authorize page.
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

/// GET /auth/outlook/callback — exchange the code and persist tokens.
pub async fn callback(
    State(app): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Redirect {
    if let Some(err) = params.error {
        let reason = params.error_description.unwrap_or(err);
        return Redirect::temporary(&format!("{SETTINGS_URL}?outlook=error&reason={}",
            urlencoding::encode(&reason)));
    }
    let (Some(code), Some(state)) = (params.code, params.state) else {
        return Redirect::temporary(&format!("{SETTINGS_URL}?outlook=error&reason=missing_code"));
    };
    if !consume_state(&app.oauth_state, &state, Utc::now()) {
        return Redirect::temporary(&format!("{SETTINGS_URL}?outlook=error&reason=bad_state"));
    }
    let tokens = match app.oauth.exchange_code(&code).await {
        Ok(t) => t,
        Err(e) => return Redirect::temporary(&format!("{SETTINGS_URL}?outlook=error&reason={}",
            urlencoding::encode(&e.to_string()))),
    };
    let uid = app.default_user_id;
    let _ = app.config_repo.set(uid, "outlook.access_token", &tokens.access_token).await;
    let _ = app.config_repo.set(uid, "outlook.token_expires_at", &tokens.expires_at.to_rfc3339()).await;
    if let Some(rt) = &tokens.refresh_token {
        let _ = app.config_repo.set(uid, "outlook.refresh_token", rt).await;
    }
    if let Some(acct) = &tokens.account {
        let _ = app.config_repo.set(uid, "outlook.account", acct).await;
    }
    Redirect::temporary(&format!("{SETTINGS_URL}?outlook=connected"))
}
```

Run: `cd backend && rg '^urlencoding' crates/api/Cargo.toml || cargo add urlencoding -p api`

- [ ] **Step 5: Wire routes + build provider in `main.rs`**

In `main.rs`, after building repos and before `build_schema`, construct OAuth + provider:

```rust
    use infrastructure::connectors::outlook::oauth::{OutlookOAuth, OutlookOAuthConfig};
    use infrastructure::connectors::outlook::token_provider::RefreshingOutlookTokenProvider;

    let oauth = Arc::new(OutlookOAuth::new(OutlookOAuthConfig {
        client_id: std::env::var("OUTLOOK_CLIENT_ID").unwrap_or_default(),
        tenant_id: std::env::var("OUTLOOK_TENANT_ID").unwrap_or_default(),
        client_secret: std::env::var("OUTLOOK_CLIENT_SECRET").unwrap_or_default(),
        redirect_uri: std::env::var("OUTLOOK_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:3001/auth/outlook/callback".to_string()),
    }));
    let outlook_token_provider: Arc<dyn application::services::OutlookTokenProvider> =
        Arc::new(RefreshingOutlookTokenProvider::new(config_repo.clone(), oauth.clone()));
```

Add `outlook_token_provider: outlook_token_provider.clone()` to the `SchemaDeps { .. }` initializer.

Add the routes and the extended state to the router:

```rust
    let default_user_id =
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let app = Router::new()
        .route("/graphql", post(graphql::schema::graphql_handler))
        .route("/graphql/playground", get(graphql::schema::graphql_playground))
        .route("/auth/outlook/login", get(auth::outlook::login))
        .route("/auth/outlook/callback", get(auth::outlook::callback))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state::AppState {
            schema: schema.clone(),
            config_repo: config_repo.clone(),
            oauth: oauth.clone(),
            default_user_id,
            oauth_state: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        });
```

Add `use uuid::Uuid;` to `main.rs` imports if not present.

- [ ] **Step 6: Build everything**

Run: `cd backend && cargo build -p api`
Expected: compiles. Fix any import/path errors surfaced.

- [ ] **Step 7: Run API tests**

Run: `cd backend && cargo test -p api`
Expected: existing GraphQL tests + new state-store tests PASS.

- [ ] **Step 8: Commit (schema + mutation + routes together)**

```bash
git add backend/crates/api backend/crates/application backend/crates/infrastructure
git commit -m "feat(api): Outlook OAuth login/callback routes + provider-backed sync"
```

---

## Task 8: `outlookConnection` query + `disconnectOutlook` mutation

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs`, `backend/crates/api/src/graphql/mutation.rs`
- (Optional) a small GraphQL type for the connection status.

- [ ] **Step 1: Add the `outlook_connection` query resolver**

In `query.rs`, add a simple object type near the top (or in `types/`):

```rust
#[derive(async_graphql::SimpleObject)]
pub struct OutlookConnectionGql {
    pub connected: bool,
    pub account: Option<String>,
}
```

Add the resolver method to `QueryRoot`:

```rust
    async fn outlook_connection(&self, ctx: &Context<'_>) -> Result<OutlookConnectionGql> {
        let user_id = ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        let refresh = config_repo.get(*user_id, "outlook.refresh_token").await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let account = config_repo.get(*user_id, "outlook.account").await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(OutlookConnectionGql {
            connected: refresh.map(|s| !s.is_empty()).unwrap_or(false),
            account,
        })
    }
```

- [ ] **Step 2: Add the `disconnect_outlook` mutation**

In `mutation.rs`:

```rust
    async fn disconnect_outlook(&self, ctx: &Context<'_>) -> Result<bool> {
        let user_id = ctx.data::<UserId>()?;
        let config_repo = ctx.data::<Arc<dyn ConfigRepository>>()?;
        for key in ["outlook.access_token", "outlook.refresh_token", "outlook.token_expires_at", "outlook.account"] {
            config_repo.set(*user_id, key, "").await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        }
        Ok(true)
    }
```

- [ ] **Step 3: Build + smoke-test the schema SDL**

Run: `cd backend && cargo run -p api -- export-schema | rg -i "outlookConnection|disconnectOutlook"`
Expected: both appear in the SDL.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/api/src/graphql
git commit -m "feat(api): outlookConnection query and disconnectOutlook mutation"
```

---

## Task 9: Frontend — Connect/Disconnect UI

**Files:**
- Modify: `frontend/src/hooks/use-settings.ts`, `frontend/src/pages/SettingsPage.tsx`

- [ ] **Step 1: Extend the settings hook with connection state + disconnect**

In `use-settings.ts`, extend `CONFIGURATION_QUERY` to also fetch the connection, and add a disconnect mutation:

```ts
const CONFIGURATION_QUERY = `
  query Configuration {
    configuration
    syncStatuses { source status lastSyncAt errorMessage }
    outlookConnection { connected account }
  }
`;

const DISCONNECT_OUTLOOK_MUTATION = `
  mutation DisconnectOutlook { disconnectOutlook }
`;
```

Add to `ConfigurationData`:

```ts
  readonly outlookConnection: { readonly connected: boolean; readonly account: string | null };
```

In `useSettings()`, add:

```ts
  const [, executeDisconnectOutlook] = useMutation<{ disconnectOutlook: boolean }>(DISCONNECT_OUTLOOK_MUTATION);
  const outlookConnection = useMemo(
    () => result.data?.outlookConnection ?? { connected: false, account: null },
    [result.data?.outlookConnection]
  );
  const disconnectOutlook = useCallback(async () => {
    const res = await executeDisconnectOutlook({});
    if (!res.error) reexecute({ requestPolicy: 'network-only' });
    return res;
  }, [executeDisconnectOutlook, reexecute]);
```

Return `outlookConnection` and `disconnectOutlook` from the hook.

- [ ] **Step 2: Replace the Access Token input with Connect/Disconnect**

In `SettingsPage.tsx`, destructure the new values from `useSettings()`. Replace the Outlook `SettingsInput` for Access Token (lines ~580-587) with:

```tsx
          {outlookConnection.connected ? (
            <div className="flex items-center justify-between rounded border p-3">
              <span className="text-sm">Connected as <strong>{outlookConnection.account ?? 'unknown'}</strong></span>
              <button
                className="rounded bg-red-600 px-3 py-1 text-white"
                onClick={async () => { await disconnectOutlook(); }}
              >
                Disconnect
              </button>
            </div>
          ) : (
            <a
              className="inline-block rounded bg-blue-600 px-4 py-2 text-white"
              href="http://localhost:3001/auth/outlook/login"
            >
              Connect Outlook
            </a>
          )}
```

Keep the "Calendar Range (days)" input and Save button (it still drives `outlook.calendar_days`). Remove the now-unused `CONFIG_KEYS.OUTLOOK_ACCESS_TOKEN` save reference from this section's `saveConfigKeys([...])` call (leave `OUTLOOK_CALENDAR_DAYS`).

- [ ] **Step 3: Show a toast on redirect-back**

Near the top of `SettingsPage()` add:

```tsx
  useEffect(() => {
    const p = new URLSearchParams(window.location.search);
    const o = p.get('outlook');
    if (o === 'connected') setSaveMessage('Outlook connected successfully.');
    else if (o === 'error') setSaveMessage(`Outlook connection failed: ${p.get('reason') ?? 'unknown'}`);
    if (o) window.history.replaceState({}, '', '/settings');
  }, []);
```

Ensure `useEffect` is imported from React.

- [ ] **Step 4: Typecheck + build**

Run: `cd frontend && pnpm build`
Expected: TypeScript compiles with no errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): Connect/Disconnect Outlook with auto-refresh status"
```

---

## Task 10: Specs (French) + security review + live verification

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md`

- [ ] **Step 1: Update SPEC_TECHNIQUE.md (French)**

Document: the two routes (`/auth/outlook/login`, `/auth/outlook/callback`), the authorization-code confidential flow, the `OUTLOOK_*` env vars, the new config keys (`outlook.refresh_token`, `outlook.token_expires_at`, `outlook.account`), the `RefreshingOutlookTokenProvider`, the `outlookConnection` query and `disconnectOutlook` mutation, and that `outlook.calendar_days` is now honored.

- [ ] **Step 2: Update SPEC_FONCTIONNELLE.md (French)**

Document the user-facing behavior: "Se connecter à Outlook" button, automatic token renewal, "Reconnect required" status when the refresh token is invalid.

- [ ] **Step 3: Commit specs**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "docs(spec): document Outlook OAuth interactive sign-in flow"
```

- [ ] **Step 4: Pre-commit security review (per CLAUDE.md)**

Dispatch the `team/security` agent on the staged auth/secret changes. Confirm: client secret never logged or returned via GraphQL; refresh token never returned via GraphQL (only a boolean `connected`); CSRF state validated + single-use; redirect bound to localhost; `.env` git-ignored.

- [ ] **Step 5: Live end-to-end verification**

1. `cd backend && cargo run -p api` and `cd frontend && pnpm dev`.
2. Open `http://localhost:3000/settings`, click **Connect Outlook**, sign in as **mbonenfant@witivio.com** (the mailbox account, NOT admin.mbonenfant), approve consent.
3. Confirm redirect to `/settings?outlook=connected` → status shows "Connected as mbonenfant@witivio.com".
4. Trigger an Outlook sync; confirm meetings populate (stale March/April ones removed via `delete_stale`).
5. Simulate expiry: in the DB set `outlook.token_expires_at` to a past timestamp, trigger sync again, confirm it silently refreshes (check `outlook.token_expires_at` advanced) and sync still succeeds.
6. Click **Disconnect**; confirm `outlookConnection.connected` becomes false and a subsequent sync reports "Reconnect required".

- [ ] **Step 6: Final verification of the original request**

Confirm the original goal — "restart from zero sync of meeting with Outlook" — is met: after connecting, the 10 stale meetings are gone and the calendar reflects the live today→+`calendar_days` window, with no manual token paste.

---

## Self-review notes

- **Spec coverage:** Entra changes (Task 0), backend routes (Task 7), token provider + refresh/rotation (Tasks 2,4), provider-backed sync (Task 6), frontend Connect/Disconnect (Task 9), `outlookConnection`/`disconnectOutlook` (Task 8), `calendar_days` fix (Task 5), specs (Task 10), security (Task 10/4). All spec sections mapped.
- **Secrets:** client secret only in `.env` (Task 0) + never serialized; refresh token never exposed over GraphQL (Task 8 returns only `connected`/`account`).
- **Type consistency:** `OutlookTokenProvider::valid_access_token(user_id) -> Result<String, AppError>` used identically in Tasks 3,4,6. `should_refresh(now, expires_at)` signature consistent (Tasks 1,4). Config keys spelled identically across Tasks 4,7,8,10 (`outlook.access_token`, `outlook.refresh_token`, `outlook.token_expires_at`, `outlook.account`, `outlook.calendar_days`).
- **Known follow-ups (out of scope):** token-at-rest encryption; reuse of the same app/flow for the Excel/SharePoint (`Files.Read.All`) connector.
```
