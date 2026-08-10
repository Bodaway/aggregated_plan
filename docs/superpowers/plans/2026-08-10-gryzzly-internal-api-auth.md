# Gryzzly Internal-API Auth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `aplan sync --source gryzzly` actually work, by replacing the non-existent API-key auth in `HttpGryzzlyClient` with the internal API's session-token method (token read from the local Chromium cookie store), RPC-style POST transport, the real endpoints, and cursor pagination.

**Architecture:** A new `GryzzlyTokenSource` trait in the application layer hides where the token comes from; infrastructure provides two implementations — `BrowserCookieTokenSource` (reads and decrypts Chromium's `remember_token` cookie) and `StaticTokenSource` (a pasted token from config, the escape hatch). `HttpGryzzlyClient` keeps its existing `GryzzlyClient` trait surface (`fetch_projects` / `fetch_tasks`) but swaps GET+Bearer for POST+`User <token>` against `view/projects.list` and `expandedProjectMetrics.get`, walking the `cursor` chain to exhaustion.

**Tech Stack:** Rust, reqwest 0.12, sqlx 0.8 (sqlite, read-only immutable for the cookie DB), `aes`/`cbc`/`pbkdf2`/`sha1` for AES-128-CBC cookie decryption, `secret-tool` shelled out for the keyring secret, `wiremock` 0.6 for HTTP tests.

**Spec:** `docs/superpowers/specs/2026-08-10-gryzzly-internal-api-auth-design.md`

## Global Constraints

- **Read-only integration.** Only `view/projects.list`, `expandedProjectMetrics.get`, and `self.getIdentity` may be called. Never implement or call `declarations.create`, `declarations.update`, `declarations.delete`, or any `timesheets.*` method. `GryzzlyClient` keeps exactly two methods.
- **Base URL:** `https://api.gryzzly.io` — no `/v1` suffix.
- **Auth header:** `Authorization: User <token>` (literal `User `, capital U, one space).
- **All API calls are `POST`** with `Content-Type: application/json`, even reads.
- **`limit` maximum is 500.** Sending 1000 is rejected with `{"ok":false,"errors":["decoding: invalid_argument: limit (out of range, max=500)"]}`.
- **`limit` is a pre-filter batch size, not a page size.** Pages come back shorter than `limit` as a matter of course. Pagination terminates **only** on `cursor == null` — never on a short or empty page.
- **Project activeness:** `status == "active" && deleted_at == null`. There is no `archived` field.
- **Task activeness:** `completed_at == null && deleted_at == null`, ANDed with project activeness.
- **Container tasks (`is_container: true`) are kept** in `gryzzly_tasks`, matching `scripts/gryzzly/import_catalog.py` today.
- DDD layering (per `CLAUDE.md`): traits in `application`, implementations in `infrastructure`. No `.unwrap()` in production paths. Map errors to `ConnectorError`.
- Tests are inline `#[cfg(test)] mod tests`. TDD: write the failing test first, watch it fail, then implement.
- Run scoped tests: `cargo test -p application`, `cargo test -p infrastructure`, `cargo test -p api`. (`crates/mcp` is excluded from the workspace and has never compiled — ignore it.)
- Commit messages: plain imperative subject, no `Co-Authored-By`, no `Signed-off-by`.

## File Structure

| File | Responsibility |
|---|---|
| `backend/crates/application/src/errors.rs` | **Modify** — add `ConnectorError::Configuration(String)` |
| `backend/crates/application/src/services/gryzzly_client.rs` | **Modify** — add the `GryzzlyTokenSource` trait |
| `backend/crates/infrastructure/Cargo.toml` | **Modify** — crypto deps + dev-deps |
| `backend/crates/infrastructure/src/connectors/gryzzly/cookie_crypto.rs` | **Create** — pure key derivation + AES-128-CBC decrypt + domain-prefix strip |
| `backend/crates/infrastructure/src/connectors/gryzzly/cookie_jar.rs` | **Create** — profile discovery, SQLite lookup, expiry check, keyring lookup |
| `backend/crates/infrastructure/src/connectors/gryzzly/token_source.rs` | **Create** — `StaticTokenSource`, `BrowserCookieTokenSource` |
| `backend/crates/infrastructure/src/connectors/gryzzly/types.rs` | **Rewrite** — real wire DTOs + envelope |
| `backend/crates/infrastructure/src/connectors/gryzzly/mapper.rs` | **Rewrite** — status/timestamp mapping + tree flattening |
| `backend/crates/infrastructure/src/connectors/gryzzly/client.rs` | **Rewrite** — POST transport, envelope handling, cursor walk |
| `backend/crates/infrastructure/src/connectors/gryzzly/mod.rs` | **Modify** — module wiring and exports |
| `backend/crates/api/src/graphql/mutation.rs` | **Modify** (~line 610-621) — build the token source, drop `gryzzly.api_key` |
| `SPEC_TECHNIQUE.md` | **Modify** — §10.6 + two config tables |
| `scripts/gryzzly/README.md` | **Modify** — reframe as fallback, fix documented limit |
| `scripts/gryzzly/export-catalog.console.js` | **Modify** — `limit: 1000` → `500` (broken today) |

Decryption is split from the cookie store deliberately: `cookie_crypto.rs` is pure and fully unit-testable, `cookie_jar.rs` touches the filesystem, SQLite and a subprocess. A reviewer can reject one without the other.

---

### Task 1: Application-layer contracts — error variant and token-source trait

Both later tasks consume these, and nothing else in the workspace matches on `ConnectorError` exhaustively (verified), so the variant addition ripples nowhere.

**Files:**
- Modify: `backend/crates/application/src/errors.rs` (the `ConnectorError` enum, ~line 45-58)
- Modify: `backend/crates/application/src/services/gryzzly_client.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `ConnectorError::Configuration(String)` — Display: `"Configuration error: {0}"`
  - `pub trait GryzzlyTokenSource: Send + Sync` with `async fn header_value(&self) -> Result<String, ConnectorError>`, exported from `application::services`

- [ ] **Step 1: Write the failing test**

Append to `backend/crates/application/src/errors.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_connector_error_displays_its_message() {
        let e = ConnectorError::Configuration("no cookie found".to_string());
        assert_eq!(e.to_string(), "Configuration error: no cookie found");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test -p application configuration_connector_error 2>&1 | tail -20`
Expected: FAIL to compile — `no variant or associated item named 'Configuration' found for enum 'ConnectorError'`

- [ ] **Step 3: Add the variant**

In `backend/crates/application/src/errors.rs`, inside `pub enum ConnectorError`, after the `ParseError` variant:

```rust
    /// The local environment is not set up for this connector: no browser cookie
    /// store found, the OS keyring is unavailable, or a stored credential expired.
    /// Distinct from `AuthFailed`, which means the remote end rejected us — and
    /// which cannot carry a detail message, since its Display is fixed.
    #[error("Configuration error: {0}")]
    Configuration(String),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd backend && cargo test -p application configuration_connector_error 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Add the token-source trait**

In `backend/crates/application/src/services/gryzzly_client.rs`, append:

```rust
/// Supplies the `Authorization` header value for the Gryzzly internal API.
///
/// Gryzzly issues no API keys. The only credential is the `remember_token`
/// session cookie minted by the Microsoft SSO login on `app.gryzzly.io`, which
/// has a fixed 7-day lifetime. This trait keeps *where that token comes from*
/// out of the HTTP client: infrastructure can read it from the local browser
/// cookie store, or take a hand-pasted value from configuration.
#[async_trait]
pub trait GryzzlyTokenSource: Send + Sync {
    /// The full header value, e.g. `User abc123…` — prefix included.
    async fn header_value(&self) -> Result<String, ConnectorError>;
}
```

Also fix the now-wrong doc comment on `GryzzlyClient` in the same file — replace `/// Read-only client for the Gryzzly v1 REST API. Named generically (not `…ReadClient`)` and its second line with:

```rust
/// Read-only client for the Gryzzly internal RPC API (`POST https://api.gryzzly.io/<method>`).
/// Read-only is a hard constraint, not an accident: the cockpit never writes declarations.
```

- [ ] **Step 6: Verify it compiles and nothing regressed**

Run: `cd backend && cargo test -p application 2>&1 | tail -15`
Expected: PASS, no warnings about the new trait

- [ ] **Step 7: Commit**

```bash
git add backend/crates/application/src/errors.rs backend/crates/application/src/services/gryzzly_client.rs
git commit -m "Add a Configuration connector error and the Gryzzly token-source trait"
```

---

### Task 2: Cookie decryption (pure)

**Files:**
- Create: `backend/crates/infrastructure/src/connectors/gryzzly/cookie_crypto.rs`
- Modify: `backend/crates/infrastructure/Cargo.toml`
- Modify: `backend/crates/infrastructure/src/connectors/gryzzly/mod.rs`

**Interfaces:**
- Consumes: `ConnectorError::{Configuration, ParseError}` (Task 1)
- Produces:
  - `pub(crate) fn derive_key(password: &[u8]) -> [u8; 16]`
  - `pub(crate) fn decrypt_value(password: &[u8], body: &[u8]) -> Result<String, ConnectorError>` — `body` is the blob **with the 3-byte version prefix already stripped**
  - `pub(crate) fn looks_like_domain_prefix(plain: &[u8]) -> bool`

- [ ] **Step 1: Add the dependencies**

In `backend/crates/infrastructure/Cargo.toml`, under `[dependencies]`:

```toml
aes = "0.8"
cbc = { version = "0.1", features = ["alloc"] }
pbkdf2 = "0.12"
sha1 = "0.10"
hmac = "0.12"
```

And add a new section at the end of the file:

```toml
[dev-dependencies]
tokio = { workspace = true }
wiremock = "0.6"
tempfile = "3"
```

Run: `cd backend && cargo build -p infrastructure 2>&1 | tail -5`
Expected: compiles (deps resolve). `wiremock 0.6` is already used by `crates/cli`, so it is in the lockfile.

- [ ] **Step 2: Write the failing test**

Create `backend/crates/infrastructure/src/connectors/gryzzly/cookie_crypto.rs` containing **only** this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};

    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

    /// Encrypt like Chromium does, so the round-trip is a real one.
    fn encrypt(password: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let key = derive_key(password);
        Aes128CbcEnc::new(&key.into(), &[0x20u8; 16].into())
            .encrypt_padded_vec_mut::<Pkcs7>(plaintext)
    }

    #[test]
    fn decrypts_a_v10_style_value() {
        let blob = encrypt(b"peanuts", b"tok3nvalue");
        assert_eq!(decrypt_value(b"peanuts", &blob).unwrap(), "tok3nvalue");
    }

    /// Newer Chromium prepends a 32-byte SHA-256 domain-binding hash to the
    /// plaintext. Confirmed present on this machine: without stripping it, the
    /// token is unusable.
    #[test]
    fn strips_the_32_byte_domain_binding_prefix() {
        let mut plain = vec![0u8; 32];
        plain[0] = 0x8f; // non-printable => recognisably a hash, not text
        plain[7] = 0x01;
        plain.extend_from_slice(b"abcdef0123456789abcdef0123456789");
        let blob = encrypt(b"peanuts", &plain);
        assert_eq!(
            decrypt_value(b"peanuts", &blob).unwrap(),
            "abcdef0123456789abcdef0123456789"
        );
    }

    /// A 32-char token with no prefix must NOT lose its first 32 bytes.
    #[test]
    fn keeps_a_bare_printable_value_intact() {
        let blob = encrypt(b"peanuts", b"abcdef0123456789abcdef0123456789");
        assert_eq!(
            decrypt_value(b"peanuts", &blob).unwrap(),
            "abcdef0123456789abcdef0123456789"
        );
    }

    #[test]
    fn wrong_password_is_a_parse_error_not_a_panic() {
        let blob = encrypt(b"peanuts", b"tok3nvalue");
        let err = decrypt_value(b"wrong-password", &blob).unwrap_err();
        assert!(
            matches!(err, ConnectorError::ParseError(_)),
            "expected ParseError, got {err:?}"
        );
    }

    #[test]
    fn prefix_detector_rejects_all_printable_leaders() {
        let printable = b"abcdef0123456789abcdef0123456789extra".to_vec();
        assert!(!looks_like_domain_prefix(&printable));
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Add `mod cookie_crypto;` to `backend/crates/infrastructure/src/connectors/gryzzly/mod.rs` (keep it above the existing `mod client;` line), then:

Run: `cd backend && cargo test -p infrastructure cookie_crypto 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find function 'derive_key'`, `cannot find function 'decrypt_value'`

- [ ] **Step 4: Write the implementation**

Prepend to `backend/crates/infrastructure/src/connectors/gryzzly/cookie_crypto.rs`, above the test module:

```rust
//! Chromium cookie-value decryption.
//!
//! Chromium stores `encrypted_value` as a 3-byte version tag followed by an
//! AES-128-CBC ciphertext. The key is PBKDF2-HMAC-SHA1 over a password that
//! depends on the tag: `v10` uses the literal `peanuts` (no keyring), `v11`
//! uses the OS keyring secret. Salt, iteration count and IV are constants
//! baked into Chromium.
//!
//! Everything here is pure so it can be tested without a browser or a keyring.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use application::errors::ConnectorError;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// Chromium's hard-coded KDF parameters.
const SALT: &[u8] = b"saltysalt";
const ROUNDS: u32 = 1;
const KEY_LEN: usize = 16;
/// Chromium's IV is sixteen spaces.
const IV: [u8; 16] = [0x20; 16];

/// Length of the SHA-256 domain-binding hash newer Chromium prepends to the plaintext.
const DOMAIN_PREFIX_LEN: usize = 32;

pub(crate) fn derive_key(password: &[u8]) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, SALT, ROUNDS, &mut key);
    key
}

/// True when the leading 32 bytes cannot be text, i.e. they are the
/// domain-binding hash rather than the start of the cookie value.
///
/// Checking printability rather than UTF-8 validity matters: a hash's bytes are
/// occasionally valid UTF-8 by luck, and silently returning 32 bytes of binary
/// glued to the token would produce a 401 that looks like an expired session.
pub(crate) fn looks_like_domain_prefix(plain: &[u8]) -> bool {
    plain.len() > DOMAIN_PREFIX_LEN
        && !plain[..DOMAIN_PREFIX_LEN]
            .iter()
            .all(|b| b.is_ascii_graphic() || *b == b' ')
}

/// Decrypt a cookie value. `body` must already have the 3-byte version tag removed.
pub(crate) fn decrypt_value(password: &[u8], body: &[u8]) -> Result<String, ConnectorError> {
    let key = derive_key(password);
    let plain = Aes128CbcDec::new(&key.into(), &IV.into())
        .decrypt_padded_vec_mut::<Pkcs7>(body)
        .map_err(|e| {
            ConnectorError::ParseError(format!(
                "cookie decryption failed ({e}) — wrong keyring secret, or Chromium changed format"
            ))
        })?;

    let start = if looks_like_domain_prefix(&plain) { DOMAIN_PREFIX_LEN } else { 0 };
    String::from_utf8(plain[start..].to_vec()).map_err(|_| {
        ConnectorError::ParseError(
            "decrypted cookie value is not UTF-8 — Chromium cookie format changed".to_string(),
        )
    })
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure cookie_crypto 2>&1 | tail -15`
Expected: PASS, 5 tests

If `pbkdf2_hmac` does not resolve, the fallback form is `pbkdf2::pbkdf2::<hmac::Hmac<sha1::Sha1>>(password, SALT, ROUNDS, &mut key).expect("pbkdf2 output length is valid")` — `hmac` is already in the deps for exactly this reason.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/infrastructure/Cargo.toml backend/Cargo.lock \
        backend/crates/infrastructure/src/connectors/gryzzly/cookie_crypto.rs \
        backend/crates/infrastructure/src/connectors/gryzzly/mod.rs
git commit -m "Decrypt Chromium cookie values, domain-binding prefix included"
```

---

### Task 3: Cookie store discovery and lookup

**Files:**
- Create: `backend/crates/infrastructure/src/connectors/gryzzly/cookie_jar.rs`
- Modify: `backend/crates/infrastructure/src/connectors/gryzzly/mod.rs`

**Interfaces:**
- Consumes: `decrypt_value`, `looks_like_domain_prefix` (Task 2); `ConnectorError::Configuration` (Task 1)
- Produces:
  - `pub(crate) enum BrowserFamily { Chromium, Chrome, Brave, Edge }` with `pub(crate) fn keyring_application(&self) -> &'static str`
  - `pub(crate) struct CookieHit { pub encrypted_value: Vec<u8>, pub expires_utc: i64, pub family: BrowserFamily, pub path: PathBuf }`
  - `pub(crate) fn candidate_stores(pinned: Option<&Path>) -> Vec<(PathBuf, BrowserFamily)>`
  - `pub(crate) async fn read_cookie(path: &Path, family: BrowserFamily) -> Option<CookieHit>`
  - `pub(crate) async fn find_remember_token(pinned: Option<&Path>) -> Result<CookieHit, ConnectorError>` — newest `expires_utc` wins; returns `Configuration` when none found
  - `pub(crate) fn check_not_expired(hit: &CookieHit, now: DateTime<Utc>) -> Result<(), ConnectorError>`
  - `pub(crate) async fn keyring_password(family: BrowserFamily) -> Result<Vec<u8>, ConnectorError>`
  - `pub(crate) async fn token_value(pinned: Option<&Path>, now: DateTime<Utc>) -> Result<String, ConnectorError>`

- [ ] **Step 1: Write the failing test**

Create `backend/crates/infrastructure/src/connectors/gryzzly/cookie_jar.rs` with **only** this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Chromium timestamps are microseconds since 1601-01-01.
    fn chromium_time(unix_secs: i64) -> i64 {
        (unix_secs + 11_644_473_600) * 1_000_000
    }

    async fn write_store(path: &Path, host: &str, name: &str, expires_utc: i64, value: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
        sqlx::query(
            "create table cookies (host_key text, name text, encrypted_value blob, expires_utc integer)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("insert into cookies values (?, ?, ?, ?)")
            .bind(host)
            .bind(name)
            .bind(value)
            .bind(expires_utc)
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn reads_a_remember_token_row() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cookies");
        write_store(&path, ".gryzzly.io", "remember_token", chromium_time(2_000_000_000), b"v11blob").await;

        let hit = read_cookie(&path, BrowserFamily::Chromium).await.expect("row found");
        assert_eq!(hit.encrypted_value, b"v11blob");
        assert_eq!(hit.expires_utc, chromium_time(2_000_000_000));
    }

    #[tokio::test]
    async fn ignores_other_cookies_on_the_same_host() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cookies");
        write_store(&path, ".gryzzly.io", "_fbp", chromium_time(2_000_000_000), b"nope").await;

        assert!(read_cookie(&path, BrowserFamily::Chromium).await.is_none());
    }

    #[tokio::test]
    async fn ignores_cookies_from_other_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cookies");
        write_store(&path, ".example.com", "remember_token", chromium_time(2_000_000_000), b"nope").await;

        assert!(read_cookie(&path, BrowserFamily::Chromium).await.is_none());
    }

    #[tokio::test]
    async fn a_missing_store_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_cookie(&dir.path().join("Cookies"), BrowserFamily::Chromium).await.is_none());
    }

    #[test]
    fn an_expired_cookie_names_its_expiry_date() {
        let hit = CookieHit {
            encrypted_value: b"x".to_vec(),
            expires_utc: chromium_time(1_754_836_890), // 2025-08-10T14:41:30Z
            family: BrowserFamily::Chromium,
            path: PathBuf::from("/tmp/Cookies"),
        };
        let now = Utc.timestamp_opt(1_786_372_890, 0).unwrap(); // a year later
        let err = check_not_expired(&hit, now).unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, ConnectorError::Configuration(_)), "got {err:?}");
        assert!(msg.contains("2025-08-10"), "expiry date missing from: {msg}");
        assert!(msg.contains("app.gryzzly.io"), "no instruction to log in again: {msg}");
    }

    #[test]
    fn a_live_cookie_passes_the_expiry_check() {
        let hit = CookieHit {
            encrypted_value: b"x".to_vec(),
            expires_utc: chromium_time(1_786_372_890),
            family: BrowserFamily::Chromium,
            path: PathBuf::from("/tmp/Cookies"),
        };
        let now = Utc.timestamp_opt(1_786_000_000, 0).unwrap();
        assert!(check_not_expired(&hit, now).is_ok());
    }

    /// A session cookie has expires_utc == 0. It cannot be validated, so accept it
    /// and let the API be the judge rather than refusing a possibly-good token.
    #[test]
    fn a_session_cookie_is_accepted() {
        let hit = CookieHit {
            encrypted_value: b"x".to_vec(),
            expires_utc: 0,
            family: BrowserFamily::Chromium,
            path: PathBuf::from("/tmp/Cookies"),
        };
        assert!(check_not_expired(&hit, Utc.timestamp_opt(1_786_000_000, 0).unwrap()).is_ok());
    }

    #[test]
    fn a_pinned_path_is_the_only_candidate() {
        let pinned = PathBuf::from("/somewhere/Custom/Cookies");
        let got = candidate_stores(Some(&pinned));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, pinned);
        assert_eq!(got[0].1, BrowserFamily::Chromium);
    }

    /// A pinned non-Chromium profile must still ask the right keyring.
    #[test]
    fn a_pinned_path_infers_its_browser_family() {
        let cases = [
            ("/home/u/.config/google-chrome/Default/Cookies", BrowserFamily::Chrome),
            ("/home/u/.config/BraveSoftware/Brave-Browser/Default/Cookies", BrowserFamily::Brave),
            ("/home/u/.config/microsoft-edge/Default/Cookies", BrowserFamily::Edge),
            ("/home/u/.config/chromium/Default/Cookies", BrowserFamily::Chromium),
        ];
        for (path, want) in cases {
            let got = candidate_stores(Some(Path::new(path)));
            assert_eq!(got[0].1, want, "wrong family for {path}");
        }
    }

    #[test]
    fn keyring_application_names_match_the_browser() {
        assert_eq!(BrowserFamily::Chromium.keyring_application(), "chromium");
        assert_eq!(BrowserFamily::Chrome.keyring_application(), "chrome");
        assert_eq!(BrowserFamily::Brave.keyring_application(), "brave");
        assert_eq!(BrowserFamily::Edge.keyring_application(), "microsoft-edge");
    }

    #[test]
    fn missing_cookie_reports_the_paths_tried() {
        let err = no_store_error(&[PathBuf::from("/a/Cookies"), PathBuf::from("/b/Cookies")]);
        let msg = err.to_string();
        assert!(msg.contains("/a/Cookies"), "paths missing from: {msg}");
        assert!(msg.contains("app.gryzzly.io"), "no instruction in: {msg}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Add `mod cookie_jar;` to `mod.rs`, then:

Run: `cd backend && cargo test -p infrastructure cookie_jar 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find type 'BrowserFamily'`, `cannot find function 'read_cookie'`

- [ ] **Step 3: Write the implementation**

Prepend to `cookie_jar.rs`:

```rust
//! Locates and reads the Gryzzly `remember_token` cookie from a local
//! Chromium-family browser profile.
//!
//! Gryzzly has no API key: the only credential is this cookie, minted by the
//! Microsoft SSO login on `app.gryzzly.io` with a fixed 7-day lifetime. Reading
//! it from disk is what lets the sync run unattended between logins.
//!
//! This is the platform-specific, fragile half of the connector — browser
//! on-disk layout and keyring encryption can both change under us. Everything
//! here fails soft (skip the candidate) or loud (`Configuration` naming the
//! problem); `gryzzly.token` in config is the escape hatch when it breaks.

use std::path::{Path, PathBuf};

use application::errors::ConnectorError;
use chrono::{DateTime, TimeZone, Utc};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use tokio::process::Command;

use super::cookie_crypto::decrypt_value;

/// Offset between the 1601-01-01 Chromium epoch and the Unix epoch, in seconds.
const CHROMIUM_EPOCH_OFFSET: i64 = 11_644_473_600;

const COOKIE_NAME: &str = "remember_token";
const HOST_SUFFIX: &str = "%gryzzly.io";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserFamily {
    Chromium,
    Chrome,
    Brave,
    Edge,
}

impl BrowserFamily {
    /// The `application` attribute `secret-tool` keys the Safe Storage secret on.
    pub(crate) fn keyring_application(&self) -> &'static str {
        match self {
            BrowserFamily::Chromium => "chromium",
            BrowserFamily::Chrome => "chrome",
            BrowserFamily::Brave => "brave",
            BrowserFamily::Edge => "microsoft-edge",
        }
    }

    /// Directory under `$XDG_CONFIG_HOME` holding this browser's profiles.
    fn config_subdir(&self) -> &'static str {
        match self {
            BrowserFamily::Chromium => "chromium",
            BrowserFamily::Chrome => "google-chrome",
            BrowserFamily::Brave => "BraveSoftware/Brave-Browser",
            BrowserFamily::Edge => "microsoft-edge",
        }
    }
}

const FAMILIES: [BrowserFamily; 4] = [
    BrowserFamily::Chromium,
    BrowserFamily::Chrome,
    BrowserFamily::Brave,
    BrowserFamily::Edge,
];

#[derive(Debug, Clone)]
pub(crate) struct CookieHit {
    pub encrypted_value: Vec<u8>,
    pub expires_utc: i64,
    pub family: BrowserFamily,
    pub path: PathBuf,
}

fn config_root() -> PathBuf {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x);
        }
    }
    match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h).join(".config"),
        Err(_) => PathBuf::from(".config"),
    }
}

/// Guess the browser family from a pinned path, so `gryzzly.cookie_profile`
/// pointing at a Chrome or Brave profile still asks the keyring for the right
/// secret. Chromium is the fallback.
fn family_from_path(path: &Path) -> BrowserFamily {
    let s = path.to_string_lossy();
    if s.contains("google-chrome") {
        BrowserFamily::Chrome
    } else if s.contains("Brave-Browser") {
        BrowserFamily::Brave
    } else if s.contains("microsoft-edge") {
        BrowserFamily::Edge
    } else {
        BrowserFamily::Chromium
    }
}

/// Every cookie store worth trying. A pinned path short-circuits discovery.
///
/// Both on-disk layouts are checked: this Chromium keeps `Cookies` at the
/// profile root, other builds put it under `Network/`.
pub(crate) fn candidate_stores(pinned: Option<&Path>) -> Vec<(PathBuf, BrowserFamily)> {
    if let Some(p) = pinned {
        return vec![(p.to_path_buf(), family_from_path(p))];
    }
    let root = config_root();
    let mut out = Vec::new();
    for family in FAMILIES {
        let base = root.join(family.config_subdir());
        let Ok(entries) = std::fs::read_dir(&base) else { continue };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            for rel in ["Cookies", "Network/Cookies"] {
                let candidate = entry.path().join(rel);
                if candidate.is_file() {
                    out.push((candidate, family));
                }
            }
        }
    }
    out
}

/// Read the `remember_token` row from one store. Any failure means "skip this
/// candidate" — a locked, corrupt or schema-changed profile must not abort the
/// search across the others.
pub(crate) async fn read_cookie(path: &Path, family: BrowserFamily) -> Option<CookieHit> {
    // `immutable` lets us read past a running browser's lock; `read_only`
    // guarantees we never write to the user's profile.
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .immutable(true);
    let pool = SqlitePool::connect_with(opts).await.ok()?;
    let row = sqlx::query(
        "select encrypted_value, expires_utc from cookies \
         where host_key like ?1 and name = ?2 order by expires_utc desc limit 1",
    )
    .bind(HOST_SUFFIX)
    .bind(COOKIE_NAME)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    pool.close().await;

    let row = row?;
    Some(CookieHit {
        encrypted_value: row.try_get::<Vec<u8>, _>("encrypted_value").ok()?,
        expires_utc: row.try_get::<i64, _>("expires_utc").ok()?,
        family,
        path: path.to_path_buf(),
    })
}

pub(crate) fn no_store_error(tried: &[PathBuf]) -> ConnectorError {
    let list = tried
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    ConnectorError::Configuration(format!(
        "no Gryzzly remember_token cookie in any browser profile (tried: {list}). \
         Log in on app.gryzzly.io, or set gryzzly.token in configuration."
    ))
}

/// Newest cookie across every candidate store wins.
pub(crate) async fn find_remember_token(pinned: Option<&Path>) -> Result<CookieHit, ConnectorError> {
    let candidates = candidate_stores(pinned);
    let mut best: Option<CookieHit> = None;
    for (path, family) in &candidates {
        if let Some(hit) = read_cookie(path, *family).await {
            if best.as_ref().map(|b| hit.expires_utc > b.expires_utc).unwrap_or(true) {
                best = Some(hit);
            }
        }
    }
    best.ok_or_else(|| no_store_error(&candidates.into_iter().map(|(p, _)| p).collect::<Vec<_>>()))
}

pub(crate) fn check_not_expired(hit: &CookieHit, now: DateTime<Utc>) -> Result<(), ConnectorError> {
    // A session cookie carries no expiry; let the API judge it.
    if hit.expires_utc == 0 {
        return Ok(());
    }
    let unix = hit.expires_utc / 1_000_000 - CHROMIUM_EPOCH_OFFSET;
    let expires = Utc
        .timestamp_opt(unix, 0)
        .single()
        .ok_or_else(|| ConnectorError::Configuration(format!("unreadable cookie expiry: {unix}")))?;
    if expires <= now {
        return Err(ConnectorError::Configuration(format!(
            "the Gryzzly session cookie expired on {} — log in again on app.gryzzly.io \
             (it lasts 7 days)",
            expires.format("%Y-%m-%d %H:%M:%S UTC")
        )));
    }
    Ok(())
}

/// The AES password: `v10` uses a literal, `v11` the OS keyring secret.
pub(crate) async fn keyring_password(family: BrowserFamily) -> Result<Vec<u8>, ConnectorError> {
    let app = family.keyring_application();
    let out = Command::new("secret-tool")
        .args(["lookup", "application", app])
        .output()
        .await
        .map_err(|e| {
            ConnectorError::Configuration(format!(
                "cannot run secret-tool to read the '{app}' Safe Storage secret ({e}). \
                 Install libsecret, or set gryzzly.token in configuration."
            ))
        })?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(ConnectorError::Configuration(format!(
            "secret-tool found no Safe Storage secret for '{app}' (exit {}). \
             Is the keyring unlocked?",
            out.status.code().unwrap_or(-1)
        )));
    }
    Ok(out.stdout)
}

/// Full path from disk to a usable cookie value: locate, check expiry, decrypt.
pub(crate) async fn token_value(
    pinned: Option<&Path>,
    now: DateTime<Utc>,
) -> Result<String, ConnectorError> {
    let hit = find_remember_token(pinned).await?;
    check_not_expired(&hit, now)?;

    if hit.encrypted_value.len() <= 3 {
        return Err(ConnectorError::ParseError(format!(
            "cookie value in {} is too short to be encrypted",
            hit.path.display()
        )));
    }
    let (version, body) = hit.encrypted_value.split_at(3);
    let password = match version {
        b"v10" => b"peanuts".to_vec(),
        b"v11" => keyring_password(hit.family).await?,
        other => {
            return Err(ConnectorError::ParseError(format!(
                "unknown Chromium cookie encryption tag {:?} in {}",
                String::from_utf8_lossy(other),
                hit.path.display()
            )))
        }
    };
    decrypt_value(&password, body)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure cookie_jar 2>&1 | tail -20`
Expected: PASS, 11 tests (a 12th is added in the next step but is `#[ignore]`d)

- [ ] **Step 5: Verify against the real cookie store on this machine**

This is the one step that proves the fragile half works. Add a temporary ignored test to `cookie_jar.rs`:

```rust
    /// Live check against the developer's own browser profile. Ignored by default
    /// because it needs a real Chromium profile, an unlocked keyring, and a
    /// current Gryzzly login.
    #[tokio::test]
    #[ignore = "requires a local Chromium profile logged into Gryzzly"]
    async fn reads_the_real_local_cookie() {
        let token = token_value(None, Utc::now()).await.expect("token");
        assert!(!token.is_empty());
        assert!(
            token.chars().all(|c| c.is_ascii_alphanumeric()),
            "token should be alphanumeric, got {} chars starting {:?}",
            token.len(),
            token.chars().take(2).collect::<String>()
        );
    }
```

Run: `cd backend && cargo test -p infrastructure reads_the_real_local_cookie -- --ignored --nocapture 2>&1 | tail -15`
Expected: PASS. The token is 32 alphanumeric characters. **If this fails, stop and diagnose before continuing** — every later task depends on it. The known-good reference: `~/.config/chromium/Default/Cookies`, `v11` tag, `secret-tool lookup application chromium` returns the secret.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/infrastructure/src/connectors/gryzzly/cookie_jar.rs \
        backend/crates/infrastructure/src/connectors/gryzzly/mod.rs
git commit -m "Find and decrypt the Gryzzly session cookie from local browser profiles"
```

---

### Task 4: Token sources

**Files:**
- Create: `backend/crates/infrastructure/src/connectors/gryzzly/token_source.rs`
- Modify: `backend/crates/infrastructure/src/connectors/gryzzly/mod.rs`

**Interfaces:**
- Consumes: `GryzzlyTokenSource` (Task 1), `cookie_jar::token_value`, `cookie_jar::find_remember_token` (Task 3)
- Produces:
  - `pub fn normalise_header(raw: &str) -> String`
  - `pub struct StaticTokenSource` with `pub fn new(raw: &str) -> Self`
  - `pub struct BrowserCookieTokenSource` with `pub fn new(pinned: Option<PathBuf>) -> Self` and `pub async fn available(&self) -> bool`
  - both implement `GryzzlyTokenSource`

- [ ] **Step 1: Write the failing test**

Create `token_source.rs` with **only** this test module:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Add `mod token_source;` to `mod.rs`, then:

Run: `cd backend && cargo test -p infrastructure token_source 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find function 'normalise_header'`

- [ ] **Step 3: Write the implementation**

Prepend to `token_source.rs`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure token_source 2>&1 | tail -15`
Expected: PASS, 7 tests

- [ ] **Step 5: Commit**

```bash
git add backend/crates/infrastructure/src/connectors/gryzzly/token_source.rs \
        backend/crates/infrastructure/src/connectors/gryzzly/mod.rs
git commit -m "Add the config-pasted and browser-cookie Gryzzly token sources"
```

---

### Task 5: Wire DTOs and mapper

Replaces the invented `archived` field with the real activeness signals, and adds the tree flattening the nested `tasks` field requires.

**Files:**
- Rewrite: `backend/crates/infrastructure/src/connectors/gryzzly/types.rs`
- Rewrite: `backend/crates/infrastructure/src/connectors/gryzzly/mapper.rs`

**Interfaces:**
- Consumes: `application::services::{GryzzlyProject, GryzzlyTask}`
- Produces:
  - `pub(crate) struct RawGryzzlyProject { id, name, customer_name, status, deleted_at }`
  - `pub(crate) struct RawGryzzlyTask { id, name, project_id, is_container, completed_at, deleted_at, tasks }`
  - `pub(crate) struct RawProjectMetrics { tasks: Option<Vec<RawGryzzlyTask>> }`
  - `pub(crate) struct Envelope<T> { ok, payload, errors, cursor }`
  - `pub(crate) fn map_project(RawGryzzlyProject) -> GryzzlyProject`
  - `pub(crate) fn map_task(RawGryzzlyTask, project_active: bool) -> GryzzlyTask`
  - `pub(crate) fn flatten_tasks(Vec<RawGryzzlyTask>, fallback_project_id: &str, depth: usize) -> Vec<RawGryzzlyTask>`

- [ ] **Step 1: Replace `types.rs` wholesale**

The old `RawList<T>` and the `archived` fields describe an API that does not exist. Overwrite `types.rs` with:

```rust
//! Wire DTOs for the Gryzzly internal API, deserializing only what the catalog
//! needs. The real project object carries 26 fields (including a large `metrics`
//! blob) and the task object 24; everything unused is dropped by serde.

use serde::Deserialize;

/// One project from `view/projects.list`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawGryzzlyProject {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Observed values: `active`, `done`. This is the only activeness signal —
    /// there is no `archived` field.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

/// One task from `expandedProjectMetrics.get`, possibly with children.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawGryzzlyTask {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub project_id: Option<String>,
    /// A grouping task, which Gryzzly refuses declarations on. Kept in the
    /// catalog anyway, matching `scripts/gryzzly/import_catalog.py` — so nothing
    /// in production reads it, and the tests assert that choice holds.
    /// (`parent_id` exists on the wire too but is deliberately not deserialized:
    /// the tree is flattened via the nested `tasks` field, so nothing needs it.)
    #[allow(dead_code)]
    #[serde(default)]
    pub is_container: Option<bool>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    /// Nested children. The API returns a tree, the catalog stores a flat list.
    #[serde(default)]
    pub tasks: Option<Vec<RawGryzzlyTask>>,
}

/// `expandedProjectMetrics.get` returns the whole project; only `tasks` is used.
/// `Default` so `post_payload` can treat a missing payload as "no tasks".
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RawProjectMetrics {
    #[serde(default)]
    pub tasks: Option<Vec<RawGryzzlyTask>>,
}

/// The envelope every internal-API method wraps its result in.
///
/// `cursor` drives pagination on list methods and is absent elsewhere. `errors`
/// arrives on failures, which come with a non-2xx status as well.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Envelope<T> {
    pub ok: bool,
    #[serde(default)]
    pub payload: Option<T>,
    #[serde(default)]
    pub errors: Option<Vec<String>>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes below are trimmed from fixtures captured against the live API.
    #[test]
    fn parses_a_real_projects_list_response() {
        let json = r#"{"ok":true,"cursor":null,"payload":[
            {"id":"p1","name":"Website","customer_name":"Acme","status":"active","deleted_at":null,
             "code":"","is_billable":true,"metrics":{"budget":{"budget_spent":0}}}
        ]}"#;
        let env: Envelope<Vec<RawGryzzlyProject>> = serde_json::from_str(json).unwrap();
        assert!(env.ok);
        assert!(env.cursor.is_none());
        let payload = env.payload.unwrap();
        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0].status.as_deref(), Some("active"));
        assert_eq!(payload[0].customer_name.as_deref(), Some("Acme"));
    }

    #[test]
    fn parses_a_failure_with_an_errors_array() {
        let json = r#"{"ok":false,"errors":["decoding: invalid_argument: limit (out of range, max=500)"]}"#;
        let env: Envelope<Vec<RawGryzzlyProject>> = serde_json::from_str(json).unwrap();
        assert!(!env.ok);
        assert_eq!(env.errors.unwrap().len(), 1);
        assert!(env.payload.is_none());
    }

    #[test]
    fn parses_a_metrics_response_and_keeps_only_tasks() {
        let json = r#"{"ok":true,"payload":{"id":"p1","name":"Website","tasks":[
            {"id":"t1","name":"Pilotage","project_id":"p1","parent_id":null,"is_container":false,
             "completed_at":null,"deleted_at":null,"planned_duration":63000}
        ]}}"#;
        let env: Envelope<RawProjectMetrics> = serde_json::from_str(json).unwrap();
        let tasks = env.payload.unwrap().tasks.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t1");
        assert_eq!(tasks[0].is_container, Some(false));
    }

    /// A cursor is a plain string on the wire, not an object.
    #[test]
    fn parses_a_non_null_cursor() {
        let json = r#"{"ok":true,"cursor":"fecdfc2c-2d53-490d-ac3a-4c09c75c4dc1","payload":[]}"#;
        let env: Envelope<Vec<RawGryzzlyProject>> = serde_json::from_str(json).unwrap();
        assert_eq!(env.cursor.as_deref(), Some("fecdfc2c-2d53-490d-ac3a-4c09c75c4dc1"));
    }
}
```

Run: `cd backend && cargo test -p infrastructure gryzzly::types 2>&1 | tail -10`
Expected: PASS, 4 tests. (`RawGryzzlyProject`/`RawProjectMetrics` are only constructed by serde here, so no other code is needed yet.)

- [ ] **Step 2: Write the failing test**

Overwrite `mapper.rs` with **only** this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn project(status: Option<&str>, deleted: Option<&str>) -> RawGryzzlyProject {
        RawGryzzlyProject {
            id: "p1".into(),
            name: " Website ".into(),
            customer_name: Some("Acme".into()),
            status: status.map(str::to_string),
            deleted_at: deleted.map(str::to_string),
        }
    }

    fn task(id: &str, completed: Option<&str>, deleted: Option<&str>) -> RawGryzzlyTask {
        RawGryzzlyTask {
            id: id.into(),
            name: format!(" {id} "),
            project_id: Some("p1".into()),
            is_container: Some(false),
            completed_at: completed.map(str::to_string),
            deleted_at: deleted.map(str::to_string),
            tasks: None,
        }
    }

    #[test]
    fn an_active_project_maps_active_and_trims_its_name() {
        let p = map_project(project(Some("active"), None));
        assert_eq!(p.id, "p1");
        assert_eq!(p.name, "Website");
        assert_eq!(p.customer_name.as_deref(), Some("Acme"));
        assert!(p.is_active);
    }

    #[test]
    fn a_done_project_maps_inactive() {
        assert!(!map_project(project(Some("done"), None)).is_active);
    }

    #[test]
    fn a_deleted_project_maps_inactive_even_when_active() {
        assert!(!map_project(project(Some("active"), Some("2026-01-01T00:00:00Z"))).is_active);
    }

    /// An absent status must not read as active: the old code defaulted an
    /// unknown flag to active and would have kept every project alive.
    #[test]
    fn a_project_without_status_maps_inactive() {
        assert!(!map_project(project(None, None)).is_active);
    }

    #[test]
    fn an_empty_customer_name_becomes_none() {
        let mut raw = project(Some("active"), None);
        raw.customer_name = Some("   ".into());
        assert_eq!(map_project(raw).customer_name, None);
    }

    #[test]
    fn an_open_task_in_an_active_project_is_active() {
        let t = map_task(task("t1", None, None), true);
        assert_eq!(t.id, "t1");
        assert_eq!(t.name, "t1");
        assert_eq!(t.project_id, "p1");
        assert!(t.is_active);
    }

    #[test]
    fn a_completed_task_is_inactive() {
        assert!(!map_task(task("t1", Some("2026-01-01T00:00:00Z"), None), true).is_active);
    }

    #[test]
    fn a_deleted_task_is_inactive() {
        assert!(!map_task(task("t1", None, Some("2026-01-01T00:00:00Z")), true).is_active);
    }

    #[test]
    fn an_open_task_in_an_inactive_project_is_inactive() {
        assert!(!map_task(task("t1", None, None), false).is_active);
    }

    #[test]
    fn flatten_walks_the_whole_tree() {
        let mut parent = task("parent", None, None);
        parent.is_container = Some(true);
        let mut child = task("child", None, None);
        child.tasks = Some(vec![task("grandchild", None, None)]);
        parent.tasks = Some(vec![child]);

        let flat = flatten_tasks(vec![parent], "p1", 0);
        let ids: Vec<&str> = flat.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["parent", "child", "grandchild"]);
    }

    /// Containers stay in the catalog, matching import_catalog.py.
    #[test]
    fn flatten_keeps_container_tasks() {
        let mut parent = task("parent", None, None);
        parent.is_container = Some(true);
        parent.tasks = Some(vec![task("child", None, None)]);

        let flat = flatten_tasks(vec![parent], "p1", 0);
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].is_container, Some(true));
    }

    #[test]
    fn a_child_without_project_id_inherits_it() {
        let mut parent = task("parent", None, None);
        let mut child = task("child", None, None);
        child.project_id = None;
        parent.tasks = Some(vec![child]);

        let flat = flatten_tasks(vec![parent], "fallback-project", 0);
        assert_eq!(flat[1].project_id.as_deref(), Some("p1"));
    }

    #[test]
    fn a_top_level_task_without_project_id_uses_the_fallback() {
        let mut orphan = task("orphan", None, None);
        orphan.project_id = None;
        let flat = flatten_tasks(vec![orphan], "fallback-project", 0);
        assert_eq!(flat[0].project_id.as_deref(), Some("fallback-project"));
    }

    #[test]
    fn flatten_stops_at_the_depth_cap() {
        // Build a chain deeper than the cap.
        let mut node = task("leaf", None, None);
        for i in 0..(MAX_TASK_DEPTH + 5) {
            let mut parent = task(&format!("n{i}"), None, None);
            parent.tasks = Some(vec![node]);
            node = parent;
        }
        let flat = flatten_tasks(vec![node], "p1", 0);
        assert!(flat.len() <= MAX_TASK_DEPTH + 1, "cap not enforced: {}", flat.len());
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd backend && cargo test -p infrastructure -- mapper 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find function 'map_project'`, `cannot find value 'MAX_TASK_DEPTH'`

- [ ] **Step 4: Write the implementation**

Prepend to `mapper.rs`:

```rust
//! Wire DTOs to application DTOs, plus the tree-to-list flattening the catalog needs.

use application::services::{GryzzlyProject, GryzzlyTask};

use super::types::{RawGryzzlyProject, RawGryzzlyTask};

/// Recursion limit for the task tree. Mirrors the depth cap in
/// `scripts/gryzzly/export-catalog.console.js`; a cycle in the API's tree would
/// otherwise be unbounded.
pub(crate) const MAX_TASK_DEPTH: usize = 50;

/// A project is active only when Gryzzly says `status: "active"` and it is not
/// soft-deleted. Observed statuses: `active`, `done`.
pub(crate) fn map_project(raw: RawGryzzlyProject) -> GryzzlyProject {
    GryzzlyProject {
        id: raw.id,
        name: raw.name.trim().to_string(),
        customer_name: raw
            .customer_name
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty()),
        is_active: raw.status.as_deref() == Some("active") && raw.deleted_at.is_none(),
    }
}

/// A task is active when neither finished nor deleted, in an active project.
pub(crate) fn map_task(raw: RawGryzzlyTask, project_active: bool) -> GryzzlyTask {
    GryzzlyTask {
        id: raw.id,
        name: raw.name.trim().to_string(),
        project_id: raw.project_id.unwrap_or_default(),
        is_active: project_active && raw.completed_at.is_none() && raw.deleted_at.is_none(),
    }
}

/// Depth-first flatten of the nested `tasks` field into one list, parents before
/// children. Children inheriting `project_id` from the parent keeps rows
/// resolvable even where the API omits it.
pub(crate) fn flatten_tasks(
    tasks: Vec<RawGryzzlyTask>,
    fallback_project_id: &str,
    depth: usize,
) -> Vec<RawGryzzlyTask> {
    if depth > MAX_TASK_DEPTH {
        return Vec::new();
    }
    let mut out = Vec::new();
    for mut task in tasks {
        let children = task.tasks.take().unwrap_or_default();
        let project_id = task
            .project_id
            .clone()
            .unwrap_or_else(|| fallback_project_id.to_string());
        task.project_id = Some(project_id.clone());
        out.push(task);
        out.extend(flatten_tasks(children, &project_id, depth + 1));
    }
    out
}
```

Import only what the mapper uses — envelope parsing is tested in `types.rs`, so pulling `Envelope` in here would just earn an unused-import warning.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure -- mapper 2>&1 | tail -20`
Expected: PASS, 14 tests

- [ ] **Step 6: Commit**

```bash
git add backend/crates/infrastructure/src/connectors/gryzzly/types.rs \
        backend/crates/infrastructure/src/connectors/gryzzly/mapper.rs
git commit -m "Map Gryzzly projects by status and flatten the real task tree"
```

---

### Task 6: POST transport and envelope handling

`client.rs` is rewritten in two tasks: transport here, the cursor walk in Task 7. This task leaves `fetch_projects`/`fetch_tasks` as thin single-call bodies so the crate compiles and is testable; Task 7 replaces `fetch_projects`.

**Files:**
- Rewrite: `backend/crates/infrastructure/src/connectors/gryzzly/client.rs`
- Modify: `backend/crates/infrastructure/src/connectors/gryzzly/mod.rs`

**Interfaces:**
- Consumes: `GryzzlyTokenSource` (Task 1), `Envelope`/`Raw*` (Task 5), `map_project`/`map_task`/`flatten_tasks` (Task 5)
- Produces:
  - `pub struct HttpGryzzlyClient` with `pub fn new(base_url: String, tokens: Arc<dyn GryzzlyTokenSource>) -> Self`
  - private `async fn post_envelope<T: DeserializeOwned>(&self, method: &str, body: &serde_json::Value) -> Result<Envelope<T>, ConnectorError>`
  - private `async fn post_payload<T: DeserializeOwned + Default>(&self, method: &str, body: &serde_json::Value) -> Result<T, ConnectorError>` — `Default` covers a missing `payload` (`Vec` and `RawProjectMetrics` both have it)

- [ ] **Step 1: Write the failing test**

Overwrite `client.rs` with **only** this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct FixedToken(&'static str);

    #[async_trait]
    impl GryzzlyTokenSource for FixedToken {
        async fn header_value(&self) -> Result<String, ConnectorError> {
            Ok(self.0.to_string())
        }
    }

    struct FailingToken;

    #[async_trait]
    impl GryzzlyTokenSource for FailingToken {
        async fn header_value(&self) -> Result<String, ConnectorError> {
            Err(ConnectorError::Configuration("no cookie".into()))
        }
    }

    fn client(server: &MockServer) -> HttpGryzzlyClient {
        HttpGryzzlyClient::new(server.uri(), Arc::new(FixedToken("User tok123")))
    }

    #[test]
    fn new_trims_a_trailing_slash() {
        let c = HttpGryzzlyClient::new(
            "https://api.gryzzly.io/".into(),
            Arc::new(FixedToken("User t")),
        );
        assert_eq!(c.base_url, "https://api.gryzzly.io");
    }

    /// Reads are POSTs here: the internal API is RPC-style. Getting this wrong
    /// is a 404, not a compile error, so it is pinned by a test.
    #[tokio::test]
    async fn posts_to_the_method_path_with_the_user_auth_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/view/projects.list"))
            .and(header("authorization", "User tok123"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "cursor": null,
                "payload": [{"id": "p1", "name": "Website", "status": "active"}]
            })))
            .mount(&server)
            .await;

        let got: Vec<RawGryzzlyProject> = client(&server)
            .post_payload("view/projects.list", &json!({"limit": 500}))
            .await
            .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "p1");
    }

    #[tokio::test]
    async fn a_401_is_an_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let err = client(&server)
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ConnectorError::AuthFailed { .. }), "got {err:?}");
    }

    #[tokio::test]
    async fn a_403_is_an_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let err = client(&server)
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ConnectorError::AuthFailed { .. }), "got {err:?}");
    }

    /// The real API answers a bad `limit` with HTTP 400 AND an `errors` array.
    /// The array is the useful part, so it must survive into the message.
    #[tokio::test]
    async fn a_400_surfaces_the_errors_array() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "ok": false,
                "errors": ["decoding: invalid_argument: limit (out of range, max=500)"]
            })))
            .mount(&server)
            .await;

        let err = client(&server)
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({"limit": 1000}))
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("max=500"), "errors array lost: {msg}");
        assert!(matches!(err, ConnectorError::Http { status: 400, .. }), "got {err:?}");
    }

    /// `ok: false` under a 200 must not be read as success.
    #[tokio::test]
    async fn a_200_with_ok_false_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": false, "errors": ["internal server error"]
            })))
            .mount(&server)
            .await;

        let err = client(&server)
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("internal server error"), "got {err}");
    }

    #[tokio::test]
    async fn a_token_source_failure_stops_before_any_request() {
        let server = MockServer::start().await;
        // No mock mounted: any request would 404 and fail differently.
        let c = HttpGryzzlyClient::new(server.uri(), Arc::new(FailingToken));
        let err = c
            .post_payload::<Vec<RawGryzzlyProject>>("view/projects.list", &json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ConnectorError::Configuration(_)), "got {err:?}");
    }

    /// The token is read once per client, not once per request: a sync makes
    /// ~20 calls and each cookie read spawns secret-tool.
    #[tokio::test]
    async fn the_token_is_fetched_once_per_client() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Counting(Arc<AtomicUsize>);

        #[async_trait]
        impl GryzzlyTokenSource for Counting {
            async fn header_value(&self) -> Result<String, ConnectorError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("User tok123".to_string())
            }
        }

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "cursor": null, "payload": []
            })))
            .mount(&server)
            .await;

        let calls = Arc::new(AtomicUsize::new(0));
        let c = HttpGryzzlyClient::new(server.uri(), Arc::new(Counting(calls.clone())));
        for _ in 0..3 {
            let _: Vec<RawGryzzlyProject> =
                c.post_payload("view/projects.list", &json!({})).await.unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test -p infrastructure gryzzly::client 2>&1 | tail -20`
Expected: FAIL to compile — `HttpGryzzlyClient` not found / `post_payload` not found

- [ ] **Step 3: Write the implementation**

Prepend to `client.rs`:

```rust
//! HTTP client for the Gryzzly internal API.
//!
//! The API is RPC-style: every method is `POST https://api.gryzzly.io/<method>`
//! with a JSON body and a `{ok, payload}` envelope — reads included. Auth is
//! `Authorization: User <session-token>`; Gryzzly issues no API keys.
//!
//! Read-only by construction: only `view/projects.list` and
//! `expandedProjectMetrics.get` are ever called.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use application::errors::ConnectorError;
use application::services::{GryzzlyClient, GryzzlyProject, GryzzlyTask, GryzzlyTokenSource};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tokio::sync::OnceCell;

use super::mapper::{flatten_tasks, map_project, map_task};
use super::types::{Envelope, RawGryzzlyProject, RawProjectMetrics};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SERVICE: &str = "gryzzly";

/// `view/projects.list` rejects anything above 500.
const PROJECTS_LIMIT: u32 = 500;

pub struct HttpGryzzlyClient {
    http: Client,
    base_url: String,
    tokens: Arc<dyn GryzzlyTokenSource>,
    /// The token is read once per client. A client lives for one sync, so this
    /// keeps a ~20-call sync from spawning `secret-tool` twenty times, while
    /// still picking up a fresh cookie on the next sync.
    header: OnceCell<String>,
}

impl HttpGryzzlyClient {
    pub fn new(base_url: String, tokens: Arc<dyn GryzzlyTokenSource>) -> Self {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build reqwest client");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            tokens,
            header: OnceCell::new(),
        }
    }

    async fn auth_header(&self) -> Result<&String, ConnectorError> {
        self.header
            .get_or_try_init(|| async { self.tokens.header_value().await })
            .await
    }

    /// Turn a failed response body into the most useful message available: the
    /// API's own `errors` array if it parses, else the raw body.
    fn error_message(body: &str) -> String {
        serde_json::from_str::<Envelope<Value>>(body)
            .ok()
            .and_then(|e| e.errors)
            .map(|errs| errs.join("; "))
            .unwrap_or_else(|| body.to_string())
    }

    async fn post_envelope<T: DeserializeOwned>(
        &self,
        method: &str,
        body: &Value,
    ) -> Result<Envelope<T>, ConnectorError> {
        let url = format!("{}/{}", self.base_url, method.trim_start_matches('/'));
        let auth = self.auth_header().await?.clone();
        let resp = self
            .http
            .post(&url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| ConnectorError::NetworkError(e.to_string()))?;

        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(ConnectorError::AuthFailed { service: SERVICE.to_string() });
        }

        let text = resp
            .text()
            .await
            .map_err(|e| ConnectorError::ParseError(e.to_string()))?;

        if !status.is_success() {
            return Err(ConnectorError::Http {
                status: status.as_u16(),
                message: Self::error_message(&text),
            });
        }

        let envelope: Envelope<T> = serde_json::from_str(&text)
            .map_err(|e| ConnectorError::ParseError(format!("{method}: {e}")))?;
        if !envelope.ok {
            return Err(ConnectorError::Http {
                status: status.as_u16(),
                message: Self::error_message(&text),
            });
        }
        Ok(envelope)
    }

    async fn post_payload<T: DeserializeOwned + Default>(
        &self,
        method: &str,
        body: &Value,
    ) -> Result<T, ConnectorError> {
        Ok(self.post_envelope::<T>(method, body).await?.payload.unwrap_or_default())
    }
}

#[async_trait]
impl GryzzlyClient for HttpGryzzlyClient {
    async fn fetch_projects(&self, active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError> {
        // Replaced by the paginated walk in the next task.
        let body = json!({"filter": "", "range": "", "search": "", "limit": PROJECTS_LIMIT});
        let raws: Vec<RawGryzzlyProject> = self.post_payload("view/projects.list", &body).await?;
        let mut projects: Vec<GryzzlyProject> = raws.into_iter().map(map_project).collect();
        if active_only {
            projects.retain(|p| p.is_active);
        }
        Ok(projects)
    }

    async fn fetch_tasks(&self, project_ids: &[String]) -> Result<Vec<GryzzlyTask>, ConnectorError> {
        let mut out = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for project_id in project_ids {
            let body = json!({"project_id": project_id});
            let metrics: RawProjectMetrics = self
                .post_payload("expandedProjectMetrics.get", &body)
                .await?;
            let flat = flatten_tasks(metrics.tasks.unwrap_or_default(), project_id, 0);
            for raw in flat {
                // Callers pass only active project ids, so project_active is true.
                let task = map_task(raw, true);
                if seen.insert(task.id.clone()) {
                    out.push(task);
                }
            }
        }
        Ok(out)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure gryzzly 2>&1 | tail -20`
Expected: PASS — the 8 client tests plus everything from Tasks 2-5

- [ ] **Step 5: Commit**

```bash
git add backend/crates/infrastructure/src/connectors/gryzzly/client.rs \
        backend/crates/infrastructure/src/connectors/gryzzly/types.rs
git commit -m "POST the Gryzzly internal API and unwrap its ok/payload envelope"
```

---

### Task 7: Cursor pagination walk

The spec's mandated regression tests. `limit` is a pre-filter batch size, so pages arrive shorter than requested as a matter of course — a walk that stops on a short page returns 4 of 37 projects and looks plausible doing it.

**Files:**
- Modify: `backend/crates/infrastructure/src/connectors/gryzzly/client.rs` (replace `fetch_projects`, add tests)

**Interfaces:**
- Consumes: `post_envelope` (Task 6)
- Produces: `fetch_projects` walking to exhaustion; `const MAX_PROJECT_PAGES: usize = 200`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `client.rs`:

```rust
    use std::sync::Mutex;
    use wiremock::{Request, Respond};

    /// Serves a scripted sequence of responses and records the request bodies,
    /// so a test can assert both the walk's results and the cursor it echoed.
    struct ScriptedPages {
        pages: Mutex<std::collections::VecDeque<serde_json::Value>>,
        seen: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    impl Respond for ScriptedPages {
        fn respond(&self, req: &Request) -> ResponseTemplate {
            self.seen
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&req.body).unwrap_or(json!(null)));
            let page = self
                .pages
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| json!({"ok": true, "payload": [], "cursor": null}));
            ResponseTemplate::new(200).set_body_json(page)
        }
    }

    fn page(ids: &[&str], cursor: Option<&str>) -> serde_json::Value {
        let payload: Vec<serde_json::Value> = ids
            .iter()
            .map(|id| json!({"id": id, "name": id, "status": "active"}))
            .collect();
        json!({"ok": true, "payload": payload, "cursor": cursor})
    }

    async fn walk_with(pages: Vec<serde_json::Value>) -> (Vec<GryzzlyProject>, Vec<serde_json::Value>) {
        let server = MockServer::start().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        Mock::given(method("POST"))
            .and(path("/view/projects.list"))
            .respond_with(ScriptedPages {
                pages: Mutex::new(pages.into_iter().collect()),
                seen: seen.clone(),
            })
            .mount(&server)
            .await;

        let got = client(&server).fetch_projects(false).await.unwrap();
        let bodies = seen.lock().unwrap().clone();
        (got, bodies)
    }

    /// THE regression test. Every page is shorter than `limit` because `limit` is
    /// a pre-filter batch size — stopping on a short page would return only "a".
    #[tokio::test]
    async fn short_pages_do_not_end_the_walk() {
        let (got, bodies) = walk_with(vec![
            page(&["a"], Some("c1")),
            page(&["b", "c"], Some("c2")),
            page(&["d"], None),
        ])
        .await;

        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c", "d"]);
        assert_eq!(bodies.len(), 3);
        assert_eq!(bodies[0].get("cursor"), None, "first request must not send a cursor");
        assert_eq!(bodies[1]["cursor"], json!("c1"));
        assert_eq!(bodies[2]["cursor"], json!("c2"));
        assert_eq!(bodies[0]["limit"], json!(500));
    }

    /// An empty page with a live cursor is not the end either: limit=2 really
    /// does return zero projects mid-walk.
    #[tokio::test]
    async fn an_empty_page_with_a_cursor_continues() {
        let (got, bodies) = walk_with(vec![
            page(&["a"], Some("c1")),
            page(&[], Some("c2")),
            page(&["b"], None),
        ])
        .await;

        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
        assert_eq!(bodies.len(), 3);
    }

    #[tokio::test]
    async fn a_single_page_with_a_null_cursor_makes_one_call() {
        let (got, bodies) = walk_with(vec![page(&["a", "b"], None)]).await;
        assert_eq!(got.len(), 2);
        assert_eq!(bodies.len(), 1);
    }

    /// An empty cursor string is as final as a null one.
    #[tokio::test]
    async fn an_empty_cursor_string_ends_the_walk() {
        let (got, bodies) = walk_with(vec![page(&["a"], Some(""))]).await;
        assert_eq!(got.len(), 1);
        assert_eq!(bodies.len(), 1);
    }

    #[tokio::test]
    async fn repeated_ids_across_pages_are_not_double_counted() {
        let (got, _) = walk_with(vec![
            page(&["a", "b"], Some("c1")),
            page(&["b", "c"], None),
        ])
        .await;

        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    /// A cursor that never nulls must error, not spin forever.
    #[tokio::test]
    async fn a_never_ending_cursor_hits_the_page_guard() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/view/projects.list"))
            // Always a fresh cursor, never null.
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "payload": [{"id": "a", "name": "a", "status": "active"}],
                "cursor": "forever"
            })))
            .mount(&server)
            .await;

        let err = client(&server).fetch_projects(false).await.unwrap_err();
        assert!(matches!(err, ConnectorError::Configuration(_)), "got {err:?}");
        assert!(err.to_string().contains("200"), "guard limit missing: {err}");
    }

    #[tokio::test]
    async fn active_only_filters_after_the_walk() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/view/projects.list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "cursor": null,
                "payload": [
                    {"id": "a", "name": "a", "status": "active"},
                    {"id": "b", "name": "b", "status": "done"}
                ]
            })))
            .mount(&server)
            .await;

        let got = client(&server).fetch_projects(true).await.unwrap();
        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["a"]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test -p infrastructure gryzzly::client 2>&1 | tail -25`
Expected: FAIL — `short_pages_do_not_end_the_walk` gets `["a"]` instead of `["a","b","c","d"]`, `bodies.len()` is 1 not 3, and `a_never_ending_cursor_hits_the_page_guard` fails because no guard exists.

- [ ] **Step 3: Replace `fetch_projects` with the walk**

In `client.rs`, add the constant next to `PROJECTS_LIMIT`:

```rust
/// Page guard for the cursor walk: 200 pages × 500 = 100k projects. It exists so
/// a server-side cursor that never nulls fails loudly instead of hanging a sync.
const MAX_PROJECT_PAGES: usize = 200;
```

and replace the whole `fetch_projects` body with:

```rust
    async fn fetch_projects(&self, active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError> {
        let mut projects: Vec<GryzzlyProject> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut cursor: Option<String> = None;
        let mut pages = 0usize;

        loop {
            if pages >= MAX_PROJECT_PAGES {
                return Err(ConnectorError::Configuration(format!(
                    "view/projects.list did not finish paginating after {MAX_PROJECT_PAGES} pages \
                     — the API kept returning a cursor"
                )));
            }
            let mut body = json!({
                "filter": "", "range": "", "search": "", "limit": PROJECTS_LIMIT
            });
            if let Some(c) = &cursor {
                body["cursor"] = json!(c);
            }

            let envelope: Envelope<Vec<RawGryzzlyProject>> =
                self.post_envelope("view/projects.list", &body).await?;
            pages += 1;

            for raw in envelope.payload.unwrap_or_default() {
                if seen.insert(raw.id.clone()) {
                    projects.push(map_project(raw));
                }
            }

            // `limit` is a pre-filter batch size, so a short or even empty page
            // says nothing about whether more data follows. Only a null (or
            // empty) cursor ends the walk.
            match envelope.cursor.filter(|c| !c.is_empty()) {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }

        if active_only {
            projects.retain(|p| p.is_active);
        }
        Ok(projects)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure gryzzly 2>&1 | tail -20`
Expected: PASS — 7 new pagination tests plus all earlier ones

- [ ] **Step 5: Export the client and check the whole crate**

Set `backend/crates/infrastructure/src/connectors/gryzzly/mod.rs` to:

```rust
mod client;
mod cookie_crypto;
mod cookie_jar;
mod mapper;
mod token_source;
mod types;

pub use client::HttpGryzzlyClient;
pub use token_source::{BrowserCookieTokenSource, StaticTokenSource};
```

Run: `cd backend && cargo clippy -p infrastructure 2>&1 | tail -20`
Expected: no warnings from the gryzzly module

- [ ] **Step 6: Commit**

```bash
git add backend/crates/infrastructure/src/connectors/gryzzly/client.rs \
        backend/crates/infrastructure/src/connectors/gryzzly/mod.rs
git commit -m "Walk the Gryzzly project cursor to exhaustion, not to the first short page"
```

---

### Task 8: Wire the token source into forceSync

**Files:**
- Modify: `backend/crates/api/src/graphql/mutation.rs` (imports ~line 20, the Gryzzly block ~line 610-621)

**Interfaces:**
- Consumes: `BrowserCookieTokenSource`, `StaticTokenSource` (Task 4), `HttpGryzzlyClient::new` (Task 6)
- Produces: nothing downstream

- [ ] **Step 1: Update the import**

Replace line 20 of `mutation.rs`:

```rust
use infrastructure::connectors::gryzzly::HttpGryzzlyClient;
```

with:

```rust
use infrastructure::connectors::gryzzly::{
    BrowserCookieTokenSource, HttpGryzzlyClient, StaticTokenSource,
};
```

- [ ] **Step 2: Replace the client-construction block**

Replace these lines (currently `mutation.rs:610-621`):

```rust
        // Build Gryzzly client from stored config.
        let gryzzly_api_key = config_repo.get(*user_id, "gryzzly.api_key").await.ok().flatten();
        let gryzzly_base_url = config_repo
            .get(*user_id, "gryzzly.base_url")
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "https://api.gryzzly.io/v1".to_string());
        let gryzzly_client: Option<Arc<dyn GryzzlyClient>> = match gryzzly_api_key {
            Some(k) if !k.is_empty() => Some(Arc::new(HttpGryzzlyClient::new(gryzzly_base_url, k))),
            _ => None,
        };
```

with:

```rust
        // Build the Gryzzly client from stored config. Gryzzly issues no API key:
        // auth is the `remember_token` session cookie from the browser login, so
        // the token source is either a hand-pasted value or the local cookie store.
        let gryzzly_base_url = config_repo
            .get(*user_id, "gryzzly.base_url")
            .await
            .ok()
            .flatten()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| "https://api.gryzzly.io".to_string());
        let manual_token = config_repo
            .get(*user_id, "gryzzly.token")
            .await
            .ok()
            .flatten()
            .filter(|t| !t.trim().is_empty());
        let cookie_profile = config_repo
            .get(*user_id, "gryzzly.cookie_profile")
            .await
            .ok()
            .flatten()
            .filter(|p| !p.trim().is_empty())
            .map(std::path::PathBuf::from);

        let gryzzly_tokens: Option<Arc<dyn GryzzlyTokenSource>> = match manual_token {
            Some(t) => Some(Arc::new(StaticTokenSource::new(&t))),
            None => {
                let source = BrowserCookieTokenSource::new(cookie_profile);
                // No cookie at all means "not configured". An *expired* cookie is
                // available, so its dated "log in again" message reaches the user
                // instead of being flattened into a bare Not configured.
                if source.available().await {
                    Some(Arc::new(source))
                } else {
                    None
                }
            }
        };
        let gryzzly_client: Option<Arc<dyn GryzzlyClient>> = gryzzly_tokens
            .map(|t| Arc::new(HttpGryzzlyClient::new(gryzzly_base_url, t)) as Arc<dyn GryzzlyClient>);
```

`GryzzlyTokenSource` resolves through the existing `use application::services::*;` at line 11.

- [ ] **Step 3: Verify the API crate compiles and its tests pass**

Run: `cd backend && cargo test -p api 2>&1 | tail -20`
Expected: PASS, no reference to `gryzzly.api_key` remains

- [ ] **Step 4: Confirm the dead config key is gone**

Run: `cd /home/mbt/appfactory/aggregated_plan && rg -n 'gryzzly\.api_key' -g '!*.db*' . | grep -v docs/superpowers`
Expected: only `SPEC_TECHNIQUE.md` hits remain (fixed in Task 9). No hits under `backend/`.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/graphql/mutation.rs
git commit -m "Build the Gryzzly client from a token source instead of an API key"
```

---

### Task 9: Documentation and the broken export script

**Files:**
- Modify: `SPEC_TECHNIQUE.md` (§10.6 around line 4371-4394, config tables around 4391 and 4917)
- Modify: `scripts/gryzzly/README.md`
- Modify: `scripts/gryzzly/export-catalog.console.js` (line 21)

- [ ] **Step 1: Fix the broken export script**

In `scripts/gryzzly/export-catalog.console.js` line 21, change `limit: 1000` to `limit: 500`. The script fails today: the API rejects 1000 with `invalid_argument: limit (out of range, max=500)`.

Note this is the fallback path's only bug fix — the script's cursor handling is out of scope, and with 37 projects one page suffices.

- [ ] **Step 2: Update `SPEC_TECHNIQUE.md` §10.6**

Rewrite the sync-flow description (French, matching the file) so it states: base URL `https://api.gryzzly.io`, POST/RPC transport, `Authorization: User <token>`, the two endpoints, `status ∈ {active, done}` for project activeness, `completed_at`/`deleted_at` for tasks, cursor pagination that only ends on a null cursor, and that `limit` maxes at 500 and is a pre-filter batch size.

Replace the config rows in **both** tables (around lines 4391 and 4917):

```markdown
| `gryzzly.base_url` | string | `https://api.gryzzly.io` | URL de base de l'API interne Gryzzly (pas de préfixe `/v1`). |
| `gryzzly.token` | string (secret) | `""` | Jeton de session collé à la main (`User <token>`). Prioritaire sur le cookie ; sert d'échappatoire si la lecture du cookie casse. |
| `gryzzly.cookie_profile` | string | `""` | Chemin absolu vers un fichier `Cookies` de profil navigateur. Vide = détection automatique. |
```

and delete both `gryzzly.api_key` rows.

Add a sentence stating that authentication uses the `remember_token` cookie from the `app.gryzzly.io` SSO login, that it lasts 7 days, and that the source reports `Not configured` when no cookie is found.

- [ ] **Step 3: Reframe `scripts/gryzzly/README.md`**

Change the "Why keyless?" section: the backend sync now works, so this tooling is the **fallback** for when the cookie read breaks (browser upgrade, keyring change, or running the API where no browser profile exists). Update the "Real API Contract" section: `Authorization: User <token>` not `Bearer <token>`, and `limit` max 500 not 1000.

- [ ] **Step 4: Verify no stale references**

Run: `cd /home/mbt/appfactory/aggregated_plan && rg -n 'gryzzly.api_key|api.gryzzly.io/v1|Bearer' SPEC_TECHNIQUE.md scripts/gryzzly/`
Expected: no hits

- [ ] **Step 5: Commit**

```bash
git add SPEC_TECHNIQUE.md scripts/gryzzly/README.md scripts/gryzzly/export-catalog.console.js
git commit -m "Document the Gryzzly internal-API auth and fix the export script limit"
```

---

### Task 10: Manual verification against the live API

Nothing before this proves the whole path works end to end. **The session cookie expires 2026-08-17 14:51:50 UTC** — if that has passed, log into `app.gryzzly.io` first.

**Files:** none (verification only)

- [ ] **Step 1: Record the pre-change catalog state**

```bash
cd /home/mbt/appfactory/aggregated_plan
sqlite3 backend/aggregated_plan.db \
  "select count(*) total, sum(is_active) active, count(distinct gryzzly_project_id) projects, max(last_synced_at) from gryzzly_tasks;"
```

Expected baseline (imported by hand on 2026-06-25): `60|58|30|2026-06-25T13:01:24…`. Write the numbers down.

- [ ] **Step 2: Confirm no stale API-key config row exists**

```bash
aplan config get gryzzly.api_key
```

Expected: unset. (Verified absent at design time — the whole `configuration` table has no `gryzzly%` row, which is why the source was always `Not configured`.)

- [ ] **Step 3: Build and restart the API**

```bash
cd backend && cargo build -p api 2>&1 | tail -5
```

Then restart the process serving port 3001 so the new binary is live. The old process predates this change and will keep reporting `Not configured`.

- [ ] **Step 4: Run the sync**

```bash
aplan sync --source gryzzly
```

Expected: success, not `Not configured`. If it reports a configuration error, read the message — it names the failure (no cookie / expired cookie with its date / keyring locked).

- [ ] **Step 5: Compare the catalog against the baseline**

```bash
sqlite3 backend/aggregated_plan.db \
  "select count(*) total, sum(is_active) active, count(distinct gryzzly_project_id) projects, max(last_synced_at) from gryzzly_tasks;"
```

Expected: `last_synced_at` is now. Projects should be **20** (the active ones) against the baseline's 30, because the old hand import included done projects — `fetch_projects(true)` filters to `status == "active"`. Task count will differ for the same reason.

Confirm nothing was destroyed rather than deactivated:

```bash
sqlite3 backend/aggregated_plan.db \
  "select count(*) from gryzzly_tasks where is_active = 0;"
```

Expected: non-zero — `soft_prune_missing` deactivates rows it no longer sees, never deletes them, so tasks already assigned to aplan tasks still resolve.

- [ ] **Step 6: Confirm assignments survived**

```bash
sqlite3 backend/aggregated_plan.db \
  "select count(*) from tasks where gryzzly_task_id is not null;"
```

Expected: unchanged from before the sync. Any aplan task pointing at a now-inactive Gryzzly task must still resolve.

- [ ] **Step 7: Verify the expired-token message**

Point the connector at a deliberately expired cookie and confirm the message is the useful one:

```bash
cd backend && cargo test -p infrastructure cookie_jar -- --nocapture 2>&1 | tail -10
```

The `an_expired_cookie_names_its_expiry_date` test already asserts the date and the "log in again on app.gryzzly.io" instruction. For a live check, set `aplan config set gryzzly.token "User definitely-not-a-real-token"` then `aplan sync --source gryzzly` — expected: an auth failure, not a panic or a silent success. Then clear it: `aplan config set gryzzly.token ""`.

- [ ] **Step 8: Full test sweep**

```bash
cd backend && cargo test -p domain -p application -p infrastructure -p api 2>&1 | tail -25
```

Expected: all green.

- [ ] **Step 9: Commit nothing, report findings**

This task produces no commit. Report the before/after catalog numbers and whether the project-count drop from 30 to 20 matches the active-only filter as predicted.

---

## Self-Review Notes

**Spec coverage:** §1 config keys → Task 8 + Task 9. §2 token source trait → Tasks 1, 4. §3 cookie jar → Tasks 2, 3. §4 transport → Task 6. §4 pagination → Task 7. §5 types/mapper → Task 5. §6 failure modes → Task 1 (variant) + Task 3 (messages) + Task 6 (HTTP mapping). Testing section → tests inside Tasks 2-7 + Task 10. Documentation → Task 9. Out-of-scope items are constrained in Global Constraints and never implemented.

**Two deviations from the spec, both deliberate:**
1. The spec described one `cookie_jar.rs`; this plan splits pure crypto into `cookie_crypto.rs` so the AES path is testable without a keyring, and so a reviewer can judge the crypto separately from the filesystem code.
2. The spec did not say how often the token is read. This plan caches it per-client in a `OnceCell`, because a client lives for exactly one sync and re-reading would spawn `secret-tool` ~20 times per sync. Pinned by `the_token_is_fetched_once_per_client`.
