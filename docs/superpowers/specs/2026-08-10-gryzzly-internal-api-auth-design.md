# Gryzzly Internal-API Auth — Replacing the Non-Existent API Key

**Date:** 2026-08-10
**Status:** Design — awaiting review
**Author:** (brainstormed with Claude)

## Problem

`HttpGryzzlyClient` (`infrastructure/src/connectors/gryzzly/client.rs`) was written against a
guessed contract: `GET {base}/projects?limit=1000` with `Authorization: Bearer <api_key>` against
`https://api.gryzzly.io/v1`. Its own comments admit the endpoints are "placeholders pending the
prerequisite probe". None of it is real — Gryzzly publishes no API key, there is no `/v1` prefix,
and the paths don't exist. The client is only constructed when the `gryzzly.api_key` config key is
non-empty (`api/src/graphql/mutation.rs:611`), so in practice the `gryzzly` sync source has always
reported `Not configured` and `aplan sync --source gryzzly` has never done anything.

The catalog in `gryzzly_tasks` is therefore maintained by hand: paste
`scripts/gryzzly/export-catalog.console.js` into the browser console on `app.gryzzly.io`, download
`gryzzly_catalog.json`, then run `scripts/gryzzly/import_catalog.py`. The DB currently holds 60
rows across 30 projects, last refreshed **2026-06-25** — six weeks stale at the time of writing.

A separate internal tool (`time-tracker`, a small React app) authenticates against the same
internal API successfully. This design ports its method into the cockpit's connector so the
`gryzzly` sync source works unattended.

## Constraint: read-only

The integration stays **strictly read-only**, as it is today. The connector calls three methods,
all reads: `view/projects.list`, `expandedProjectMetrics.get`, and `self.getIdentity` (connectivity
probe only). They are HTTP `POST` because Gryzzly's internal API is RPC-style — every method is a
POST, reads included — not because anything is written. The mutating methods the internal API
exposes (`declarations.create`, `declarations.update`, `declarations.delete`) are **out of scope**:
they are not called and the connector does not contain them. `GryzzlyClient` keeps exactly its two
existing methods, `fetch_projects` and `fetch_tasks`.

## The real contract (verified live, 2026-08-10)

Confirmed by probing the live API with the local session token. Fixtures captured for the three
methods; the field lists below are observed, not inferred.

Base URL: `https://api.gryzzly.io` — no `/v1`.
Auth header: `Authorization: User <remember_token>`.
Every call: `POST`, `Content-Type: application/json`, JSON body.
Envelope: `{ok: bool, payload: …}`, plus `cursor` on list methods. On failure the body is
`{"ok": false, "errors": ["decoding: invalid_argument: limit (out of range, max=500)"]}`.

### `view/projects.list`

Body `{"filter": "", "range": "", "search": "", "limit": 500}`. **`limit` caps at 500** — 1000 is
rejected with the error above, which means `scripts/gryzzly/export-catalog.console.js` is broken
today: it sends `limit: 1000`. Returns 37 projects for this account, `cursor: null`.

`payload` is an array of project objects with 26 fields. The ones used here: `id`, `name`,
`customer_name`, `status`, `deleted_at` — matching what `GryzzlyProject` and the `gryzzly_tasks`
table actually hold; `code` is present on the wire but nothing consumes it, so it is not
deserialized. **There is no `archived` field** — `mapper.rs` currently maps one that does not exist. `status` is the activeness signal, observed values `active` (20) and
`done` (17).

#### Pagination, and the `limit` trap

Verified by walking the chain at `limit: 10`: the request parameter is **`cursor`**, carrying back
the `cursor` value from the previous response. The walk ran 11 pages and recovered all 37 projects
with zero duplicates and zero omissions, matching the single `limit: 500` call exactly. Of the
candidate names tried (`after`, `next`, `offset`, `from`, `page_token`), all were **silently
ignored** — the API returned the same first page and the same cursor — so unknown parameters fail
open, not loudly. `range` is a real parameter with its own format and returns HTTP 500 if given a
UUID.

**`limit` is a pre-filter batch size, not a page size.** Those 11 pages returned 4, 5, 4, 4, 3, 3,
4, 3, 2, 3, 2 projects — every page shorter than the requested 10. The API evidently fetches up to
`limit` rows, then filters (visibility, permissions, soft-deletes) before responding. Two
consequences:

- A short page does **not** mean the end of the data. Terminating on `payload.len() < limit` — the
  natural thing to write — would return **4 of 37** projects.
- An empty page does not mean the end either: `limit: 2` returned one project with a non-null
  cursor, and following it returned zero projects with another non-null cursor.

**Only `cursor == null` terminates the walk.**

### `expandedProjectMetrics.get`

Body `{"project_id": "<uuid>"}`. Envelope `{ok, payload}`, no `cursor`. `payload` is the full
project object plus `tasks`, `tasks_metrics`, `purchases`, `discounts`, `user_rates`.

`payload.tasks` is a tree: each task carries a nested `tasks` array of children. Task fields used:
`id`, `name`, `project_id`, `parent_id`, `is_container`, `completed_at`, `deleted_at`, `tasks`. A
task is active when both `completed_at` and `deleted_at` are null — the same rule the console
script already applies.

### `self.getIdentity`

Body `{}`. `payload` has `user`, `team`, `preferences`, `subscription`, `access_control`,
`team_analytics`, `user_analytics`. Used only to verify auth end to end without a full sync.

## The token

`remember_token`, a cookie on `.gryzzly.io` set after Microsoft SSO login on `app.gryzzly.io`.
Measured on this machine:

- 32 characters after decryption.
- **Fixed 7-day TTL, not rolling**: created 2026-08-03 14:41:30, expires 2026-08-10 14:41:30, and
  `last_update_utc == creation_utc` — using the app does not extend it. Confirmed twice: a fresh
  login the same day produced created 2026-08-10 14:51:50 / expires 2026-08-17 14:51:50, again with
  `last_update_utc == creation_utc`. So: log into Gryzzly once a week, and syncs work for seven days.
- `is_httponly = 0` (which is why the time-tracker's `document.cookie` bookmarklet works),
  `is_secure = 1`.
- Present in exactly one browser profile here: `~/.config/chromium/Default/Cookies`. Note the file
  sits at the **profile root**, not under `Network/`. The other six Chromium profiles and the
  Firefox profile have no Gryzzly cookie.
- `encrypted_value` carries the `v11` prefix, so the AES key derives from the OS keyring secret
  `Chromium Safe Storage`. `gnome-keyring-daemon` runs with the `secrets` component and
  `secret-tool lookup application chromium` returns it.
- The decrypted plaintext carries a **32-byte domain-binding SHA-256 prefix** ahead of the value,
  which newer Chromium builds prepend. It must be stripped.

The cookie is read from the local browser profile rather than pasted into config. That is the more
fragile of the two options — it depends on Chromium's on-disk layout and keyring encryption — so
the design keeps a manual paste path as an explicit escape hatch (§2).

## Architecture

### 1. Config keys

`gryzzly.api_key` is **deleted**, not renamed: there is no API key in this scheme.

| Key | Default | Role |
|---|---|---|
| `gryzzly.base_url` | `https://api.gryzzly.io` | was `https://api.gryzzly.io/v1`, which does not exist |
| `gryzzly.token` | `""` | optional manual override — the token, with or without the `User ` prefix |
| `gryzzly.cookie_profile` | `""` | optional absolute path to one browser `Cookies` file; empty = auto-detect |

### 2. `GryzzlyTokenSource`

New trait in `application/src/services/gryzzly_client.rs`:

```rust
#[async_trait]
pub trait GryzzlyTokenSource: Send + Sync {
    /// The full `Authorization` header value, e.g. `User abc123…`.
    async fn header_value(&self) -> Result<String, ConnectorError>;
}
```

`HttpGryzzlyClient::new(base_url: String, api_key: String)` becomes
`new(base_url: String, tokens: Arc<dyn GryzzlyTokenSource>)`.

Two implementations in `infrastructure/src/connectors/gryzzly/token_source.rs`:

- `StaticTokenSource(String)` — returns the `gryzzly.token` config value, normalising a bare token
  to `User <token>` (same normalisation the time-tracker's `setToken` does).
- `BrowserCookieTokenSource` — reads and decrypts the cookie (§3).

`mutation.rs` selects: `gryzzly.token` if non-empty, else `BrowserCookieTokenSource`, else `None`
so the source reports `Not configured`.

The trait earns its place three ways: the OS-specific fragility sits behind one interface,
`HttpGryzzlyClient` becomes testable with a fake token source, and the paste-a-token escape hatch
falls out for free when the cookie route breaks (browser change, or running the API on a host with
no browser profile).

### 3. `connectors/gryzzly/cookie_jar.rs`

All the fragile, platform-specific code in one file, with a pure core:

1. **Discover** candidate cookie stores. If `gryzzly.cookie_profile` is set, use only that. Else
   glob `$XDG_CONFIG_HOME` (default `~/.config`) over
   `{chromium, google-chrome, BraveSoftware/Brave-Browser, microsoft-edge}/*/Cookies` **and**
   `…/*/Network/Cookies` — this Chromium keeps the file at the profile root, other builds use
   `Network/`, so both layouts are checked.
2. **Open** each candidate with sqlx: `SqliteConnectOptions::new().filename(p).read_only(true)
   .immutable(true)`. `immutable` means a running browser's lock cannot block the read, and no new
   DB dependency is needed. On failure, skip that candidate.
3. **Query** `SELECT encrypted_value, expires_utc FROM cookies WHERE host_key LIKE '%gryzzly.io'
   AND name = 'remember_token'`. Across all candidates, keep the row with the latest
   `expires_utc`.
4. **Reject expired**: `expires_utc` is microseconds since 1601-01-01, so
   `expires_utc / 1_000_000 - 11_644_473_600` is a Unix timestamp. If it is in the past, return
   `ConnectorError::Configuration` (§6) whose message names the expiry date.
5. **Decrypt**:
   - `v11` prefix → password = stdout of `secret-tool lookup application <browser>`, where
     `<browser>` is `chromium`, `chrome`, `brave`, or `microsoft-edge` per the profile's family.
   - `v10` prefix → password = literal `peanuts`.
   - Key = PBKDF2-HMAC-SHA1(password, salt `saltysalt`, 1 iteration, 16 bytes).
   - AES-128-CBC, IV = sixteen `0x20` bytes. Strip PKCS#7 padding.
   - If the result is not valid UTF-8, drop the leading 32 bytes (the domain-binding hash) and
     retry. Confirmed necessary on this machine.
6. Return `User <value>`.

New dependencies on `infrastructure`: `aes`, `cbc`, `pbkdf2`, `hmac`, `sha1` — all pure Rust, no C
toolchain. `secret-tool` is shelled out rather than pulling in a D-Bus/Secret-Service stack,
following the existing `ShellGitConnector` precedent for shelling out to a local binary.

The decrypt step is factored as a pure function so it is unit-testable without a keyring:

```rust
pub(crate) fn decrypt_value(version: &[u8], password: &[u8], body: &[u8]) -> Result<String, ConnectorError>;
```

### 4. Transport — `client.rs`

`get_json` is replaced by:

```rust
async fn post_json<T: DeserializeOwned>(&self, method: &str, body: &serde_json::Value) -> Result<T, ConnectorError>;
```

- `POST {base_url}/{method}`, headers `Content-Type: application/json` and
  `Authorization: <token from the source>`.
- Deserialize `Envelope<T> { ok: bool, payload: Option<T>, errors: Option<Vec<String>>, cursor: Option<serde_json::Value> }`.
- `ok == false` → `ConnectorError::Http { status, message: errors.join("; ") }`.
- `401` / `403` → `ConnectorError::AuthFailed { service: "gryzzly" }`, unchanged. This is the
  mid-flight expiry case, distinct from the pre-flight expiry check in §3 step 4.
- Other non-2xx → `ConnectorError::Http` with the body, unchanged.
- Request timeout stays 30s.

`fetch_projects(active_only)`: walks `view/projects.list` to exhaustion.

```
body = {"filter": "", "range": "", "search": "", "limit": 500}
loop:
    if cursor is set: body["cursor"] = cursor
    response = post("view/projects.list", body)
    accumulate response.payload
    cursor = response.cursor
    if cursor is null: break
```

The loop terminates **only** on `cursor == null` — never on a short or empty page, for the reasons
in the contract section above. A page-count guard of 200 iterations (100k projects at `limit: 500`)
turns a server-side cursor that never nulls into a `ConnectorError::Configuration` rather than an
infinite sync; it should never fire. Ids are deduplicated as they accumulate, so a server-side
cursor that repeats a page cannot double-count.

For this account the first page returns all 37 projects with `cursor: null`, so the loop makes one
call today. The walk exists for when the team outgrows 500 *pre-filter* rows, which is invisible
from the response length.

`fetch_tasks(project_ids)`: one `expandedProjectMetrics.get {"project_id": id}` per id, sequential,
reusing the existing loop. `sync_gryzzly` calls `fetch_projects(true)` first and passes only active
ids (`application/src/use_cases/sync.rs:501`), so this is 20 calls, not 37.

### 5. Types and mapper

`types.rs` — DTOs deserialize only the fields used; the real project object has 26 fields including
a large `metrics` blob, all ignored.

```rust
struct RawGryzzlyProject { id, name, code: Option<String>, customer_name: Option<String>,
                           status: Option<String>, deleted_at: Option<String> }
struct RawGryzzlyTask   { id, name, project_id: Option<String>, parent_id: Option<String>,
                          is_container: Option<bool>, completed_at: Option<String>,
                          deleted_at: Option<String>, tasks: Option<Vec<RawGryzzlyTask>> }
struct RawProjectMetrics { tasks: Option<Vec<RawGryzzlyTask>> }
```

`RawList<T>` is deleted — the envelope replaces it.

`mapper.rs`:

- `map_project`: `is_active = status.as_deref() == Some("active") && deleted_at.is_none()`. The
  `archived` field and its two tests go away.
- `map_task`: `is_active = project_active && completed_at.is_none() && deleted_at.is_none()`.
- New pure `flatten_tasks(tasks, fallback_project_id, depth) -> Vec<RawGryzzlyTask>`, recursing
  through the nested `tasks` field, depth-capped at 50, falling back to the parent's `project_id`
  when a child omits it. Mirrors the console script's `flat()`.

**Container tasks are kept.** Tasks with `is_container: true` are stored in `gryzzly_tasks` exactly
as `import_catalog.py` stores them today, so the swap stays directly comparable to the 60 rows
already in the DB. `is_container` is deserialized but not acted on; filtering them out of the
picker is a possible follow-up, not part of this change.

### 6. Failure modes

`ConnectorError` today has four variants — `Http { status, message }`, `AuthFailed { service }`,
`NetworkError(String)`, `ParseError(String)` — and none fits "the local environment is not set up".
`AuthFailed` cannot carry detail at all: its `Display` is a fixed
`"Authentication failed for {service}"`. So this design adds one variant:

```rust
#[error("Configuration error: {0}")]
Configuration(String),
```

Nothing in the workspace matches on `ConnectorError` exhaustively — it is only ever constructed and
`to_string()`'d — so the addition ripples nowhere. It matters because `sync_gryzzly` funnels
`e.to_string()` into `sync_status.error_message`, which is the text the user actually reads.

| Condition | Result |
|---|---|
| No cookie in any profile, no `gryzzly.token` | client is `None` → `Not configured` |
| Cookie found, `expires_utc` past | `Configuration`, naming the expiry date and saying to log into `app.gryzzly.io` again |
| `secret-tool` fails / keyring locked | `Configuration`, naming the keyring secret and the exit status |
| Cookie file unreadable in every candidate profile | `Configuration`, listing the paths tried |
| Decrypt yields non-UTF-8 even after stripping 32 bytes | `ParseError` — signals a Chromium format change |
| API returns 401/403 mid-sync | `AuthFailed { service: "gryzzly" }` (token died between the check and the call) |
| API returns `ok: false` | `Http` with the joined `errors` array |

Given the fixed 7-day TTL, the expired-cookie path is expected roughly weekly and its message is
the one the user will actually read.

## Testing

Pure units, unit-tested:

- `decrypt_value`: a `v10` round-trip (password `peanuts` is deterministic, so a fixture can be
  built in-test), a case with the 32-byte domain-binding prefix, a bad-padding case.
- `map_project`: `active` → active, `done` → inactive, `deleted_at` set → inactive, missing
  `status` → inactive.
- `map_task`: both timestamps null → active, either set → inactive, inactive project → inactive.
- `flatten_tasks`: nested tree flattens fully, child inherits `project_id`, depth cap holds,
  container rows survive.
- Envelope parsing: `ok: true` with payload, `ok: false` with `errors`, against the captured
  fixtures for all three methods.
- The pagination walk, against a scripted fake transport. These are the regression tests for the
  `limit` trap and must exist:
  - three pages, each **shorter than `limit`**, the last with `cursor: null` → all rows returned
    (a walk that stopped on the first short page would return only the first page);
  - a middle page that is **empty** but carries a non-null cursor → the walk continues;
  - a page whose ids repeat an earlier page → no double-counting;
  - a cursor that never nulls → the 200-iteration guard fires as `Configuration`, not a hang.

Integration:

- `cookie_jar` profile discovery and expired-row rejection against a temp SQLite file built in the
  test, covering both the profile-root and `Network/` layouts.
- `HttpGryzzlyClient` against a fake `GryzzlyTokenSource`, asserting the request method, path, and
  `Authorization` header.

Manual gate: `aplan sync --source gryzzly`, then compare `gryzzly_tasks` against the 60 rows / 30
projects currently present, and confirm `sync_status(gryzzly)` reports `success`. Then re-run after
letting the token expire (or with a deliberately expired fixture) and confirm the message names the
expiry date.

## Documentation

- `SPEC_TECHNIQUE.md` §10.6 — rewrite the sync flow against the real endpoints; correct both config
  tables (around lines 4391 and 4917) to drop `gryzzly.api_key` and add `gryzzly.token` /
  `gryzzly.cookie_profile`; correct the `base_url` default.
- `scripts/gryzzly/README.md` — reframe as the fallback path now that the sync source works, and
  fix its documented `limit: 1000`.
- `scripts/gryzzly/export-catalog.console.js` — change `limit: 1000` to `500`; it is broken today.

## Out of scope

- Any write to Gryzzly. `declarations.create` / `.update` / `.delete` are not implemented.
- Reading declarations back (`timesheets.get`, `timesheets.getDay`,
  `forms.declarations.listProjects`, `forms.declarations.listTasks`).
- Filtering container tasks out of the cockpit's task picker.
- Automatic re-login when the 7-day cookie expires. The user logs into Gryzzly in the browser as
  they already do; the connector only reads what that login leaves behind.
