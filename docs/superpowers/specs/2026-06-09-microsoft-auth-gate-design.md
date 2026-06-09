# Microsoft Sign-In Gate — App-wide Auth + Unified Graph Token

**Date:** 2026-06-09
**Status:** Design — awaiting review
**Supersedes:** `2026-06-08-outlook-oauth-integration-design.md` (Outlook-only). That work (branch `feat/outlook-oauth-integration`, 14 commits) is the foundation; this generalizes it.

## Problem

The previous design scoped OAuth to Outlook only, behind a per-source "Connect" button in Settings. That is the wrong shape: the same Microsoft Graph identity also powers the Excel/SharePoint connector (`Files.Read.All`), and the user wants authentication to happen **once at app startup**, not per integration. The app should be gated by a single Microsoft sign-in whose token serves **all** Graph integrations.

## Decisions (confirmed with user)

1. **Hard login gate.** On startup, if there is no valid Microsoft session, the app shows a "Sign in with Microsoft" screen and is otherwise unusable. The Microsoft sign-in is the app's front door.
2. **Single-user session gate (not full multi-user).** Sign-in unlocks the local cockpit and provides the Graph token; the backend stays single-user (the existing `DEFAULT_USER_ID` owns all data). The gate is a UX/session unlock, **not** per-request backend authentication (the API stays bound to `127.0.0.1`, CORS-locked).
3. **One token for all Graph integrations.** A single delegated token with `Calendars.Read` + `Files.Read.All` + `offline_access` (+ `openid profile`) is stored, auto-refreshed, and used by **both** the Outlook and Excel/SharePoint connectors.
4. **Admin consent granted tenant-wide.** Using the admin `az` session (`admin.mbonenfant@witivio.com`, has `DelegatedPermissionGrant.ReadWrite.All`), grant admin consent for the app's delegated scopes so sign-in is frictionless and `Files.Read.All` works without per-user prompts.
5. **Rename Outlook→Microsoft/Graph** across the code built in the prior branch.

## Fixed parameters

| Item | Value |
|------|-------|
| Tenant ID | `0ca0e5b0-fbba-4994-839d-8d47b96d86db` (Witivio) |
| Client ID | `12dd5cbd-f897-4184-a473-8effc7a93aba` ("Aggregated Plan") |
| Authority | `https://login.microsoftonline.com/{tenant}` |
| Scopes | `https://graph.microsoft.com/Calendars.Read https://graph.microsoft.com/Files.Read.All offline_access openid profile` |
| Redirect URI | `http://localhost:3001/auth/microsoft/callback` (Web platform) |
| Flow | Authorization Code, confidential client + secret |
| Account | sign in as the **mailbox** user `mbonenfant@witivio.com` (not `admin.mbonenfant`, which has no mailbox) |

## Entra app configuration (one-time, via admin `az`)

1. Ensure delegated Graph permissions on app `12dd5cbd`: `Calendars.Read`, `Files.Read.All`, `offline_access`, `openid`, `profile` (Calendars.Read/offline_access/Files.Read.All/User.Read already present; add openid/profile if missing).
2. Add **Web** redirect URI `http://localhost:3001/auth/microsoft/callback`.
3. **Grant admin consent** for those delegated scopes on the app's service principal (create/patch the tenant-wide `oauth2PermissionGrant` for the Microsoft Graph resource SP with consentType `AllPrincipals`).
4. Reuse the existing client secret already stored in `backend/.env`.

## Architecture

### Component 1 — Backend OAuth (crate `api` + `infrastructure`), generalized

- `infrastructure/.../microsoft/oauth.rs` (renamed from `outlook/oauth.rs`): `MicrosoftOAuth`, `MicrosoftOAuthConfig`, `TokenSet`, `should_refresh`. `scope()` returns the combined scope string above.
- Routes (renamed): `GET /auth/microsoft/login`, `GET /auth/microsoft/callback`. Same behavior — CSRF `state` (single-use, 10-min TTL, eviction on insert), code exchange with secret, persist tokens, redirect to the SPA. On error → redirect to SPA root with `?auth=error&reason=...`. On success → `?auth=connected` (or just the SPA root).
- Config keys (renamed): `microsoft.refresh_token`, `microsoft.access_token`, `microsoft.token_expires_at`, `microsoft.account`.

### Component 2 — Unified token provider (`application` + `infrastructure`)

- `application`: trait `GraphTokenProvider { async fn valid_access_token(&self, user_id) -> Result<String, AppError> }` (renamed from `OutlookTokenProvider`).
- `infrastructure`: `RefreshingGraphTokenProvider` (renamed), reads/writes the `microsoft.*` keys, refreshes via the refresh token (60s skew), rotates the refresh token. On refresh failure → `AppError::Connector` with message `"Sign-in required"`.
- **Both connectors use it.** In `force_sync`, build `GraphOutlookClient` *and* `GraphExcelClient` from `provider.valid_access_token(user_id)`. Excel's previously-static token wiring is replaced.

### Component 3 — Session query + sign-out (GraphQL)

- Query `session { authenticated account }` — `authenticated = (microsoft.refresh_token present & non-empty)`, `account = microsoft.account`. Reads the refresh token server-side (not via the redacted `configuration` query).
- Mutation `signOut` — clears the four `microsoft.*` keys; returns `true`.

### Component 4 — Frontend hard login gate

- A top-level `AuthGate` wrapper around the app: on mount, runs the `session` query.
  - `fetching` → minimal loading state.
  - `!authenticated` → full-screen **"Sign in with Microsoft"** page; the button navigates to `http://localhost:3001/auth/microsoft/login`.
  - `authenticated` → render the app. The header/nav shows "Signed in as `<account>`" + a **Sign out** button (calls `signOut`, then re-checks session → returns to the gate).
- On return from the callback (`?auth=connected|error`), the gate re-runs `session` and clears the query string.
- Settings loses the per-source token/Connect controls; the Outlook/Excel sections no longer ask for tokens (they rely on the global session). The "Calendar Range (days)" input stays.

### Component 5 — Env + specs

- `backend/.env` / `.env.example`: `MICROSOFT_CLIENT_ID`, `MICROSOFT_TENANT_ID`, `MICROSOFT_CLIENT_SECRET`, `MICROSOFT_REDIRECT_URI` (migrate the existing `OUTLOOK_*` values).
- Update `SPEC_FONCTIONNELLE.md` / `SPEC_TECHNIQUE.md` (French) to describe the startup sign-in gate and the unified Graph token.

## Data flow

```
App load → SPA queries `session`
  authenticated=false → "Sign in with Microsoft" screen
    → GET /auth/microsoft/login → 302 → Microsoft sign-in (admin-consented, frictionless)
    → 302 → GET /auth/microsoft/callback?code&state → exchange (secret) → store microsoft.* tokens
    → 302 → SPA root?auth=connected → SPA re-queries `session` → authenticated=true → app renders

Any Graph sync (Outlook or Excel):
  force_sync → provider.valid_access_token() (refresh if near expiry, rotate)
    → GraphOutlookClient / GraphExcelClient built with the fresh token → fetch
```

## Error handling

- **Consent/sign-in denied or state mismatch:** callback redirects to SPA root with `?auth=error&reason=...`; gate shows the reason and the sign-in button again.
- **Refresh token expired/revoked:** when the token endpoint returns a definitive `invalid_grant` (HTTP 400 with that error code), the provider clears the stored `microsoft.refresh_token` (and access token) and returns `"Sign-in required"` — so the next `session` check reports `authenticated=false` and the gate reappears. **Transient** failures (network errors, 5xx) do NOT clear the token; they just surface `"Sign-in required"` for that sync attempt and leave the session intact to retry.
- **Wrong account (no mailbox, e.g. admin.mbonenfant):** Graph returns `MailboxNotEnabledForRESTAPI`; surfaced in Outlook sync status. (Excel is unaffected by mailbox state.)

## Security

- Client secret only in `backend/.env` (git-ignored, chmod 600), never logged or returned via GraphQL.
- Refresh/access tokens in the `configuration` table (plaintext SQLite) — accepted local-MVP risk; the `configuration` GraphQL query redacts secret-like keys to `********`.
- CSRF `state`: UUID v4, single-use, 10-min TTL, evicted on insert.
- CORS locked to `http://localhost:3000`; API bound to `127.0.0.1`.
- The login gate is a UX/session unlock, not a backend authorization boundary — acceptable for a localhost single-user cockpit; explicitly out of scope to add per-request auth.

## Testing

- **Unit (pure):** `should_refresh`, authorize-URL builder, `state` store add/validate/expire/evict (already exist; renamed).
- **Unit (mocked/fake):** token-provider refresh decision + rotation + "Sign-in required" (already exist; renamed).
- **Frontend:** `AuthGate` renders the sign-in screen when `session.authenticated=false` and the app when true (component test with a mocked urql client).
- **Manual E2E:** sign in once → app unlocks → Outlook sync clears the 10 stale meetings and repopulates → (if SharePoint configured) Excel sync uses the same token → Sign out returns to the gate.

## Out of scope (YAGNI)

- Full per-user authentication / multi-user data partitioning (single-user gate only).
- Token encryption at rest (tracked for later).
- Per-request backend auth / session cookies (gate is client-side + token-presence).
- Proactive background refresh (refresh stays lazy, on sync / on session check).
