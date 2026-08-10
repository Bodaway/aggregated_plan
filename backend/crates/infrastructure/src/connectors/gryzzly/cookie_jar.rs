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

#[cfg(test)]
mod tests {
    // `TimeZone` arrives via the parent module's imports.
    use super::*;

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
            expires_utc: chromium_time(1_754_836_890), // 2025-08-10
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

    /// Live check against the developer's own browser profile. Ignored by default
    /// because it needs a real Chromium profile, an unlocked keyring, and a
    /// current Gryzzly login.
    #[tokio::test]
    #[ignore = "requires a local Chromium profile logged into Gryzzly"]
    async fn reads_the_real_local_cookie() {
        let token = token_value(None, Utc::now()).await.expect("token");
        // Printable-ASCII is the invariant that matters: it is exactly what fails
        // when the 32-byte domain-binding prefix is left on, since a SHA-256 hash
        // is binary. The token is not purely alphanumeric — it contains hyphens.
        assert!(token.len() >= 16, "token implausibly short: {} chars", token.len());
        assert!(
            token.chars().all(|c| c.is_ascii_graphic()),
            "token should be printable ASCII, got {} chars",
            token.len()
        );
    }

    #[test]
    fn missing_cookie_reports_the_paths_tried() {
        let err = no_store_error(&[PathBuf::from("/a/Cookies"), PathBuf::from("/b/Cookies")]);
        let msg = err.to_string();
        assert!(msg.contains("/a/Cookies"), "paths missing from: {msg}");
        assert!(msg.contains("app.gryzzly.io"), "no instruction in: {msg}");
    }
}
