# Outlook OAuth Integration — Interactive Sign-in + Auto-Refresh

**Date:** 2026-06-08
**Status:** Design — awaiting review
**Author:** (brainstormed with Claude)

## Problem

Outlook calendar sync depends on a Microsoft Graph access token pasted manually into
Settings (`outlook.access_token`). Graph access tokens expire in ~1 hour, there is no
refresh mechanism, and the token had been dead since 2026-03-30 — so calendar sync silently
stopped working. The user wants a real integration: sign in once interactively, and have the
backend keep the token alive automatically. No more pasting.

## Feasibility (already proven via live probes, 2026-06-08)

These were validated against the live Witivio tenant before designing:

- Borrowing first-party clients is **blocked** by tenant policy: Azure CLI lacks calendar
  scope (`AADSTS65002`), Microsoft Graph PowerShell requires admin consent, Graph Explorer is
  a confidential client (no public-client flow).
- The tenant **allows ordinary users to register apps** (`allowedToCreateApps: true`) and to
  **self-consent `Calendars.Read`** on their own single-tenant app (no admin approval).
- A registered app successfully returned a **refresh token**, and that refresh token minted a
  fresh access token (with rotation). The end-to-end refresh loop works.
- `/me/calendarView` succeeds for a mailbox-enabled account. (The admin account
  `admin.mbonenfant@witivio.com` has no REST mailbox → `MailboxNotEnabledForRESTAPI`; the
  primary `mbonenfant@witivio.com` is the correct identity.)

## Fixed Parameters

| Item | Value |
|------|-------|
| Tenant ID | `0ca0e5b0-fbba-4994-839d-8d47b96d86db` (Witivio) |
| App (client_id) | `12dd5cbd-f897-4184-a473-8effc7a93aba` ("Aggregated Plan") |
| Authority | `https://login.microsoftonline.com/0ca0e5b0-...` |
| Authorize endpoint | `{authority}/oauth2/v2.0/authorize` |
| Token endpoint | `{authority}/oauth2/v2.0/token` |
| Scopes | `https://graph.microsoft.com/Calendars.Read offline_access openid profile` |
| Redirect URI | `http://localhost:3001/auth/outlook/callback` |
| Flow | Authorization Code, **confidential client + client secret** |
| Graph call (unchanged) | `GET /me/calendarView` (delegated) |

## Entra App Changes (one-time, prerequisite)

Applied to app `12dd5cbd` (owner = the user):

1. Add delegated Microsoft Graph permissions **`Calendars.Read`** + **`offline_access`**
   (keep existing `Files.Read.All`, `User.Read`).
2. Register a **Web** redirect URI `http://localhost:3001/auth/outlook/callback`
   (http allowed because it is loopback/localhost).
3. Create a **client secret**; store it in backend `.env` as `OUTLOOK_CLIENT_SECRET`
   (never committed; add a placeholder to `.env.example`).

## Architecture

### Component 1 — Backend OAuth endpoints (crate `api`)

New module `api/src/auth/outlook.rs`, two Axum routes added to the router in `main.rs`:

- `GET /auth/outlook/login`
  - Generates a random `state` (CSRF) and stores it in a short-lived in-memory store in
    `AppState` (`state -> created_at`, TTL ~10 min).
  - Builds the authorize URL (client_id, redirect_uri, `response_type=code`,
    `response_mode=query`, scope, state).
  - Returns `302` to Microsoft.
- `GET /auth/outlook/callback?code&state`
  - Validates `state` against the store (reject unknown/expired; single-use).
  - On `error`/`error_description` query params → redirect to
    `http://localhost:3000/settings?outlook=error&reason=...`.
  - Exchanges `code` at the token endpoint (`grant_type=authorization_code`, client_id,
    client_secret, redirect_uri, code).
  - Persists `outlook.refresh_token`, `outlook.access_token`,
    `outlook.token_expires_at` (RFC3339), and signed-in `outlook.account` (from id_token upn)
    via `ConfigRepository`.
  - Redirects to `http://localhost:3000/settings?outlook=connected`.

State store: a `Mutex<HashMap<String, DateTime<Utc>>>` held in `AppState`. Simple, single-user,
no new dependency.

### Component 2 — Token provider (crates `application` + `infrastructure`)

- `application`: new trait `OutlookTokenProvider { async fn valid_access_token(&self, user_id) -> Result<String, AppError> }`.
- `infrastructure`: `RefreshingOutlookTokenProvider` implementing it:
  - Reads `outlook.access_token` + `outlook.token_expires_at` from config.
  - If `now >= expires_at - 60s` (or missing): POST `grant_type=refresh_token` with stored
    refresh token + client secret; on success update stored access token, expiry, and the
    **rotated** refresh token; return the new access token.
  - On refresh failure (revoked/expired refresh token): set Outlook `SyncStatus` to error
    `"Reconnect required"` and return `AppError::Connector { Outlook, ... }`.
- `force_sync` (mutation) and `sync_source` build the `GraphOutlookClient` from
  `provider.valid_access_token(...)` instead of reading the static `outlook.access_token`.
  `GraphOutlookClient::new(token)` is unchanged — it still takes a bearer string; the provider
  guarantees freshness before construction.

**App-level static config (backend `.env`):** `OUTLOOK_CLIENT_ID`, `OUTLOOK_TENANT_ID`,
`OUTLOOK_CLIENT_SECRET`, `OUTLOOK_REDIRECT_URI` (with the fixed defaults above; secret has no
default). These are not per-user and never go in the database.

**Per-user tokens (`configuration` table, via `ConfigRepository`):** `outlook.refresh_token`,
`outlook.access_token`, `outlook.token_expires_at`, `outlook.account`.
`outlook.access_token` remains as a key but is now machine-managed, not user-entered.

### Component 3 — Frontend Settings (crate `frontend`)

- Replace the "Access Token" password input with:
  - A **Connect Outlook** button → navigates to `http://localhost:3001/auth/outlook/login`.
  - A status line: "Connected as `<outlook.account>`" + a **Disconnect** button when connected;
    "Not connected" otherwise.
- Connection status derived from a new lightweight GraphQL query
  `outlookConnection { connected account }` (reads config presence), or reuse existing config
  fetch. Disconnect = mutation clearing the outlook token keys.
- On landing back at `/settings?outlook=connected|error`, show a toast and refresh status.

### Component 4 — Spec + adjacent fix

- Update `SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md` (French) for the new auth flow, routes,
  and config keys.
- Fix the latent bug: `sync_source` hardcodes a 30-day window (`sync.rs:660`) and ignores
  `outlook.calendar_days`. Make it read the config (default 14, matching the UI default).

## Data Flow

```
User clicks "Connect Outlook"
  → GET /auth/outlook/login  → 302 → Microsoft authorize (interactive sign-in + consent)
  → 302 back to GET /auth/outlook/callback?code&state
  → backend exchanges code (with client_secret) → {access, refresh, expires_in}
  → store tokens in configuration → 302 → frontend /settings?outlook=connected

Later, on sync:
  force_sync → provider.valid_access_token()
    → if expired: POST refresh_token grant → new access (+ rotated refresh) → store
    → return fresh access token
  → GraphOutlookClient.fetch_calendar(/me/calendarView) → upsert + delete_stale
```

## Error Handling

- **User denies consent / state mismatch:** callback redirects to Settings with an error
  reason; no tokens stored.
- **Refresh token expired or revoked (e.g. 90-day inactivity, password change, admin revoke):**
  provider surfaces `Reconnect required` as the Outlook sync status; UI prompts re-Connect.
- **Wrong account (no mailbox):** unchanged Graph behaviour returns
  `MailboxNotEnabledForRESTAPI`; surface the message in sync status so the user knows to use
  their primary account.

## Security

- **Client secret**: stored only in backend `.env` (`OUTLOOK_CLIENT_SECRET`), git-ignored;
  placeholder in `.env.example`. Never logged.
- **Refresh token**: stored in the `configuration` table (plaintext SQLite), consistent with
  the existing `jira.token`. Accepted risk for the local single-user MVP; flagged for a future
  at-rest-encryption pass. Never logged or returned over GraphQL.
- **CSRF**: `state` parameter validated server-side, single-use, short TTL.
- **Redirect**: callback bound to localhost only; tokens never placed in redirect URLs.
- Pre-commit **team/security** review required (auth + secrets change), per project conventions.

## Testing Strategy

- **Unit (pure):** token-expiry decision (`should_refresh(now, expires_at)`), authorize-URL
  builder, `state` store add/validate/expire — no I/O.
- **Unit (mocked HTTP):** refresh-grant success updates stored tokens + rotates refresh;
  refresh failure maps to `Reconnect required`. Mock the token endpoint.
- **Integration:** callback handler with a stubbed token endpoint (state validation,
  config persistence, redirect target). Full live OAuth is validated manually (already proven).
- Follow TDD: tests before implementation, per project conventions.

## Out of Scope (YAGNI)

- Multi-user / multi-account Outlook connections (schema is multi-user ready, but the cockpit
  is single-user locally).
- Encrypting tokens at rest (tracked as future work).
- Reusing this flow for the Excel/SharePoint connector (`Files.Read.All` already on the app) —
  natural follow-up once the calendar flow is proven, but not in this change.
- Background/scheduled proactive refresh — refresh happens lazily on sync, which is sufficient.
