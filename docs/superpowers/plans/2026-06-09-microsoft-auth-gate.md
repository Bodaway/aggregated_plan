# Microsoft Sign-In Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the per-Outlook "Connect" button with a single Microsoft sign-in gate at app startup whose one Graph token (`Calendars.Read` + `Files.Read.All` + `offline_access`) powers both the Outlook and Excel/SharePoint connectors.

**Architecture:** Generalize the existing Outlook authorization-code + refresh-token machinery (branch `feat/outlook-oauth-integration`) to a Microsoft/Graph-wide flow; add a frontend `AuthGate` that blocks the app until a valid session exists; wire both connectors to one `GraphTokenProvider`. Single-user; the gate is a client-side session unlock, not per-request backend auth.

**Tech Stack:** Rust (Axum 0.7, async-graphql 7, sqlx, reqwest, async_trait, thiserror, chrono), React 18 + urql + TypeScript, Microsoft Graph v1.0.

**Spec:** `docs/superpowers/specs/2026-06-09-microsoft-auth-gate-design.md`

---

## Starting point

The branch `feat/outlook-oauth-integration` already implements (Outlook-named): `OutlookOAuth`/`OutlookOAuthConfig`/`TokenSet`/`should_refresh` (`infrastructure/.../outlook/oauth.rs`), `RefreshingOutlookTokenProvider` (`.../outlook/token_provider.rs`), `OutlookTokenProvider` trait (`application/services/outlook_token_provider.rs`), `/auth/outlook/*` routes + CSRF store (`api/src/auth/outlook.rs`), extended `AppState` (`api/src/state.rs`), provider injection (`api/src/graphql/schema.rs`), `force_sync` using the provider + `outlook_connection` query + `disconnect_outlook` mutation + `is_secret_key` redaction (`api/src/graphql/{mutation,query}.rs`), and the frontend Settings Connect/Disconnect UI. All backend tests pass; `GraphExcelClient::new(access_token)` and `GraphOutlookClient::new(access_token)` both exist.

## Branch

- [ ] **Rename the working branch**

```bash
cd /home/mbt/appfactory/aggregated_plan
git branch -m feat/outlook-oauth-integration feat/microsoft-auth-gate
git branch --show-current   # expect feat/microsoft-auth-gate
```

---

## Task 1: Entra app config + admin consent + env rename (prerequisite)

**Run via the admin `az` session** (`admin.mbonenfant@witivio.com`, has `Application.ReadWrite.All` + `DelegatedPermissionGrant.ReadWrite.All`). Token: `az account get-access-token --resource-type ms-graph --tenant 0ca0e5b0-fbba-4994-839d-8d47b96d86db --query accessToken -o tsv`.

**Files:** `backend/.env` (local), `backend/.env.example`

- [ ] **Step 1: Ensure scopes + add the Web redirect URI**

PATCH app `12dd5cbd` so `requiredResourceAccess` for Graph (`00000003-0000-0000-c000-000000000000`) includes all four delegated scopes and add the Web redirect:

```bash
TENANT=0ca0e5b0-fbba-4994-839d-8d47b96d86db
AT=$(az account get-access-token --resource-type ms-graph --tenant $TENANT --query accessToken -o tsv)
OBJ=$(curl -s -H "Authorization: Bearer $AT" "https://graph.microsoft.com/v1.0/applications?\$filter=appId%20eq%20'12dd5cbd-f897-4184-a473-8effc7a93aba'&\$select=id" | python3 -c "import sys,json;print(json.load(sys.stdin)['value'][0]['id'])")
curl -s -X PATCH "https://graph.microsoft.com/v1.0/applications/$OBJ" -H "Authorization: Bearer $AT" -H "Content-Type: application/json" -d '{
  "web": { "redirectUris": ["http://localhost:3001/auth/microsoft/callback"] },
  "requiredResourceAccess": [{ "resourceAppId": "00000003-0000-0000-c000-000000000000", "resourceAccess": [
    {"id":"465a38f9-76ea-45b9-9f34-9e8b0d4b0b42","type":"Scope"},
    {"id":"df85f4d6-205c-4ac5-a5ea-6bf408dba283","type":"Scope"},
    {"id":"7427e0e9-2fba-42fe-b0c0-848c9e6a8182","type":"Scope"},
    {"id":"e1fe6dd8-ba31-4d61-89e7-88639da4683d","type":"Scope"},
    {"id":"37f7f235-527c-4136-accd-4a02d197296e","type":"Scope"},
    {"id":"14dad69e-099b-42c9-810b-d002981feec1","type":"Scope"} ]}] }'
```

(IDs: `465a38f9`=Calendars.Read, `df85f4d6`=Files.Read.All, `7427e0e9`=offline_access, `e1fe6dd8`=User.Read, `37f7f235`=openid, `14dad69e`=profile.)
Verify: `curl -s -H "Authorization: Bearer $AT" "https://graph.microsoft.com/v1.0/applications/$OBJ?\$select=web,requiredResourceAccess" | python3 -m json.tool` shows the Web redirect + 6 scopes.

- [ ] **Step 2: Grant admin consent tenant-wide (delegated)**

Get the app's service principal id and the Graph SP id, then create an `oauth2PermissionGrant` with `consentType=AllPrincipals`:

```bash
APP_SP=$(curl -s -H "Authorization: Bearer $AT" "https://graph.microsoft.com/v1.0/servicePrincipals?\$filter=appId%20eq%20'12dd5cbd-f897-4184-a473-8effc7a93aba'&\$select=id" | python3 -c "import sys,json;print(json.load(sys.stdin)['value'][0]['id'])")
GRAPH_SP=$(curl -s -H "Authorization: Bearer $AT" "https://graph.microsoft.com/v1.0/servicePrincipals?\$filter=appId%20eq%20'00000003-0000-0000-c000-000000000000'&\$select=id" | python3 -c "import sys,json;print(json.load(sys.stdin)['value'][0]['id'])")
curl -s -X POST "https://graph.microsoft.com/v1.0/oauth2PermissionGrants" -H "Authorization: Bearer $AT" -H "Content-Type: application/json" -d "{
  \"clientId\":\"$APP_SP\", \"consentType\":\"AllPrincipals\", \"resourceId\":\"$GRAPH_SP\",
  \"scope\":\"Calendars.Read Files.Read.All offline_access openid profile User.Read\" }"
```

If a grant already exists (409/duplicate), PATCH it instead to include the full scope string. Verify: `curl -s -H "Authorization: Bearer $AT" "https://graph.microsoft.com/v1.0/servicePrincipals/$APP_SP/oauth2PermissionGrants"` shows the scopes with `consentType: AllPrincipals`.

- [ ] **Step 3: Rename env keys**

Edit `backend/.env` (keep the existing secret value): rename `OUTLOOK_CLIENT_ID/TENANT_ID/CLIENT_SECRET` → `MICROSOFT_CLIENT_ID/TENANT_ID/CLIENT_SECRET`, and `OUTLOOK_REDIRECT_URI` → `MICROSOFT_REDIRECT_URI=http://localhost:3001/auth/microsoft/callback`. Mirror the rename in `backend/.env.example` (blank secret). `chmod 600 backend/.env`.

- [ ] **Step 4: Commit the example**

```bash
git add backend/.env.example
git commit -m "chore(env): rename Outlook OAuth env vars to MICROSOFT_*"
```

---

## Task 2: Backend rename Outlook → Microsoft/Graph + add Files.Read.All scope

Cohesive refactor: the existing test suite is the safety net (behavior is unchanged except the scope string). Keep `cargo test` green at the end.

**Files (rename + edit):**
- `infrastructure/src/connectors/outlook/oauth.rs` → keep file, but the OAuth type is now Graph-wide. Create `infrastructure/src/connectors/microsoft/` module OR rename in place. **Decision: rename the directory** `connectors/outlook/` stays for the calendar *client* (`client.rs`, `mapper.rs`, `types.rs`), but move OAuth + token provider to a new `connectors/microsoft/` module since they're shared by Excel too.

- [ ] **Step 1: Create `connectors/microsoft/` and move OAuth + token provider**

```bash
cd /home/mbt/appfactory/aggregated_plan/backend
mkdir -p crates/infrastructure/src/connectors/microsoft
git mv crates/infrastructure/src/connectors/outlook/oauth.rs crates/infrastructure/src/connectors/microsoft/oauth.rs
git mv crates/infrastructure/src/connectors/outlook/token_provider.rs crates/infrastructure/src/connectors/microsoft/token_provider.rs
```

Create `crates/infrastructure/src/connectors/microsoft/mod.rs`:

```rust
pub mod oauth;
pub mod token_provider;
```

In `crates/infrastructure/src/connectors/mod.rs`, add `pub mod microsoft;` and remove the `pub mod oauth;`/`pub mod token_provider;` lines from `connectors/outlook/mod.rs`.

- [ ] **Step 2: Rename the OAuth types + widen the scope**

In `microsoft/oauth.rs`: rename `OutlookOAuth`→`MicrosoftOAuth`, `OutlookOAuthConfig`→`MicrosoftOAuthConfig`. Change `scope()` to return:

```rust
    pub fn scope(&self) -> &'static str {
        "https://graph.microsoft.com/Calendars.Read https://graph.microsoft.com/Files.Read.All offline_access openid profile"
    }
```

Update the test `authorize_url_contains_required_params` to also assert `url.contains("Files.Read.All")`. Keep all other tests.

- [ ] **Step 3: Rename the token provider + trait**

In `microsoft/token_provider.rs`: rename `RefreshingOutlookTokenProvider`→`RefreshingGraphTokenProvider`; it implements the renamed trait (next), reads/writes `microsoft.*` config keys (was `outlook.*`), and its "Reconnect required" message becomes `"Sign-in required"`. Add: on a refresh response that is HTTP 400 with body containing `invalid_grant`, clear `microsoft.refresh_token` and `microsoft.access_token` (set to "") before returning the error. (Surface this by having `MicrosoftOAuth::refresh` return a distinguishable error, e.g. map `invalid_grant` to a dedicated `ConnectorError::AuthFailed`; in the provider, on `AuthFailed` clear the keys.)

`git mv crates/application/src/services/outlook_token_provider.rs crates/application/src/services/graph_token_provider.rs`; rename trait `OutlookTokenProvider`→`GraphTokenProvider`; update `services/mod.rs` (`pub mod graph_token_provider; pub use graph_token_provider::*;`).

- [ ] **Step 4: Rename config keys everywhere**

Replace these exact strings across the backend: `outlook.access_token`→`microsoft.access_token`, `outlook.refresh_token`→`microsoft.refresh_token`, `outlook.token_expires_at`→`microsoft.token_expires_at`, `outlook.account`→`microsoft.account`. (Leave `outlook.calendar_days` as-is — it's calendar-specific config, not auth.) Find them with: `rg -n "outlook\.(access_token|refresh_token|token_expires_at|account)" crates/`.

- [ ] **Step 5: Rename routes, AppState, env reads, handlers**

- `git mv crates/api/src/auth/outlook.rs crates/api/src/auth/microsoft.rs`; in `auth/mod.rs` change `pub mod outlook;`→`pub mod microsoft;`.
- In `auth/microsoft.rs`: `SETTINGS_URL` callback redirect params stay, but redirect to the SPA root for the gate: success → `http://localhost:3000/?auth=connected`, error → `http://localhost:3000/?auth=error&reason=...`.
- `main.rs`: update imports to `infrastructure::connectors::microsoft::oauth::{MicrosoftOAuth, MicrosoftOAuthConfig}` and `::token_provider::RefreshingGraphTokenProvider`; read `MICROSOFT_*` env vars (default redirect `http://localhost:3001/auth/microsoft/callback`); rename the `outlook_token_provider` binding to `graph_token_provider` (typed `Arc<dyn application::services::GraphTokenProvider>`); routes become `/auth/microsoft/login` + `/auth/microsoft/callback`.
- `state.rs`: `oauth: Arc<MicrosoftOAuth>` (renamed type).
- `schema.rs`: `SchemaDeps.outlook_token_provider`→`graph_token_provider: Arc<dyn GraphTokenProvider>`; `.data(graph_token_provider)`.
- `mutation.rs` `force_sync`: `ctx.data::<Arc<dyn GraphTokenProvider>>()?` (renamed).

- [ ] **Step 6: Build + test green**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api`
Expected: all pass (same counts as before, tests renamed). Then `cargo clippy -p infrastructure -p application -p api 2>&1 | tail -20` — no errors.
Run: `rg -n "Outlook(OAuth|TokenProvider)|outlook_token_provider|/auth/outlook|outlook\.(access|refresh|token_expires|account)" crates/` → expect **no matches** (all renamed). `GraphOutlookClient` and the `outlook` calendar client module stay (those are legitimately the calendar connector).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor(auth): generalize Outlook OAuth to Microsoft Graph (scope adds Files.Read.All)"
```

---

## Task 3: Wire the Excel connector to the Graph token provider

**Files:** `backend/crates/api/src/graphql/mutation.rs` (`force_sync`)

- [ ] **Step 1: Build the Excel client from the provider token**

Replace the excel_client block in `force_sync`:

```rust
        let excel_client: Option<Arc<dyn ExcelClient>> = ctx
            .data::<Arc<dyn ExcelClient>>()
            .ok()
            .cloned();
```

with (reusing the token already fetched for Outlook — fetch once, share):

```rust
        // One Graph token serves both connectors.
        let graph_token_provider = ctx.data::<Arc<dyn GraphTokenProvider>>()?;
        let graph_token = graph_token_provider.valid_access_token(*user_id).await.ok();
        let outlook_client: Option<Arc<dyn OutlookClient>> = graph_token
            .clone()
            .map(|t| Arc::new(GraphOutlookClient::new(t)) as Arc<dyn OutlookClient>);
        let excel_client: Option<Arc<dyn ExcelClient>> = graph_token
            .map(|t| Arc::new(GraphExcelClient::new(t)) as Arc<dyn ExcelClient>);
```

Remove the now-duplicated separate `outlook_token_provider`/`outlook_client` block from Task 2 Step 5 (this replaces it — there must be exactly ONE token fetch). Add `use infrastructure::connectors::excel::GraphExcelClient;` if not already imported (check existing imports first).

- [ ] **Step 2: Build + test**

Run: `cd backend && cargo build -p api && cargo test -p api`
Expected: compiles, tests pass. `cargo clippy -p api` clean.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/api/src/graphql/mutation.rs
git commit -m "feat(sync): Excel connector uses the unified Graph token"
```

---

## Task 4: `session` query + `signOut` mutation (rename from outlook_connection/disconnect)

**Files:** `backend/crates/api/src/graphql/query.rs`, `mutation.rs`

- [ ] **Step 1: Rename the connection query to `session`**

In `query.rs`: rename `OutlookConnectionGql`→`SessionGql { authenticated: bool, account: Option<String> }` and the resolver `outlook_connection`→`session`. It reads `microsoft.refresh_token` directly (server-side, not the redacted query): `authenticated = Some(v) if !v.is_empty()`, `account = microsoft.account`.

- [ ] **Step 2: Rename the disconnect mutation to `sign_out`**

In `mutation.rs`: rename `disconnect_outlook`→`sign_out`; it clears `microsoft.access_token`, `microsoft.refresh_token`, `microsoft.token_expires_at`, `microsoft.account` (set ""), returns `true`.

- [ ] **Step 3: Build + verify SDL**

Run: `cd backend && cargo build -p api && cargo run -p api -- export-schema | rg -i "session|signOut"`
Expected: SDL shows `session: SessionGql!`, `type SessionGql { authenticated: Boolean! account: String }`, and `signOut: Boolean!`. `cargo test -p api` passes.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/api/src/graphql
git commit -m "feat(api): session query and signOut mutation (Microsoft sign-in gate)"
```

---

## Task 5: Frontend — AuthGate + header sign-out + Settings cleanup

**Files:**
- Create: `frontend/src/components/auth/AuthGate.tsx`
- Create: `frontend/src/hooks/use-session.ts`
- Modify: `frontend/src/main.tsx`, `frontend/src/components/layout/PageLayout.tsx` (header), `frontend/src/pages/SettingsPage.tsx`, `frontend/src/hooks/use-settings.ts`

- [ ] **Step 1: Session hook**

Create `frontend/src/hooks/use-session.ts`:

```ts
import { useCallback } from 'react';
import { useMutation, useQuery } from 'urql';

const SESSION_QUERY = `query Session { session { authenticated account } }`;
const SIGN_OUT_MUTATION = `mutation SignOut { signOut }`;

export interface SessionData { authenticated: boolean; account: string | null; }

export function useSession() {
  const [result, reexecute] = useQuery<{ session: SessionData }>({ query: SESSION_QUERY });
  const [, executeSignOut] = useMutation<{ signOut: boolean }>(SIGN_OUT_MUTATION);
  const signOut = useCallback(async () => {
    await executeSignOut({});
    reexecute({ requestPolicy: 'network-only' });
  }, [executeSignOut, reexecute]);
  return {
    session: result.data?.session ?? { authenticated: false, account: null },
    fetching: result.fetching,
    error: result.error ?? null,
    refresh: () => reexecute({ requestPolicy: 'network-only' }),
    signOut,
  };
}
```

- [ ] **Step 2: AuthGate component**

Create `frontend/src/components/auth/AuthGate.tsx`:

```tsx
import { useEffect } from 'react';
import { useSession } from '@/hooks/use-session';

const LOGIN_URL = 'http://localhost:3001/auth/microsoft/login';

export function AuthGate({ children }: { children: React.ReactNode }) {
  const { session, fetching, refresh } = useSession();

  useEffect(() => {
    const p = new URLSearchParams(window.location.search);
    if (p.get('auth')) {
      window.history.replaceState({}, '', window.location.pathname);
      refresh();
    }
  }, [refresh]);

  if (fetching) {
    return <div className="flex h-screen items-center justify-center text-sm text-muted-foreground">Loading…</div>;
  }
  if (!session.authenticated) {
    const reason = new URLSearchParams(window.location.search).get('reason');
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-4">
        <h1 className="text-xl font-semibold">Aggregated Plan</h1>
        <p className="text-sm text-muted-foreground">Sign in with your Microsoft account to continue.</p>
        {reason && <p className="text-sm text-red-600">Sign-in failed: {reason}</p>}
        <a className="rounded bg-blue-600 px-4 py-2 text-white hover:bg-blue-700 transition-colors" href={LOGIN_URL}>
          Sign in with Microsoft
        </a>
      </div>
    );
  }
  return <>{children}</>;
}
```

- [ ] **Step 3: Wrap the app**

In `frontend/src/main.tsx`, wrap `<App />` with `<AuthGate>`:

```tsx
import { AuthGate } from '@/components/auth/AuthGate';
// ...
    <Provider value={urqlClient}>
      <AuthGate>
        <App />
      </AuthGate>
    </Provider>
```

- [ ] **Step 4: Header sign-out**

In `frontend/src/components/layout/PageLayout.tsx`, import `useSession` and render the account + a Sign out button in the header bar:

```tsx
// inside the header JSX:
const { session, signOut } = useSession();
// ...
{session.authenticated && (
  <div className="flex items-center gap-2 text-sm">
    <span className="text-muted-foreground">{session.account}</span>
    <button className="rounded border px-2 py-1 hover:bg-accent" onClick={() => signOut()}>Sign out</button>
  </div>
)}
```

(Place it in the existing header/topbar element; match the file's current layout. If PageLayout has no header bar element, add a minimal right-aligned `<div>` at the top.)

- [ ] **Step 5: Settings cleanup**

In `frontend/src/pages/SettingsPage.tsx`: remove the Outlook Connect/Disconnect block and `outlookConnection`/`disconnectOutlook` usage from `use-settings.ts` (they're replaced by the gate). Keep the "Calendar Range (days)" input. The Microsoft Graph section now just shows informational text like "Signed in via the app sign-in." Remove the `outlookConnection` field from `use-settings.ts`'s query/`ConfigurationData` and the `disconnectOutlook` mutation. Keep the `"********"` skip-on-save guard.

- [ ] **Step 6: Build**

Run: `cd frontend && pnpm build`
Expected: TypeScript compiles, no errors. No remaining references to `outlookConnection`/`disconnectOutlook`: `rg -n "outlookConnection|disconnectOutlook" src` → no matches.

- [ ] **Step 7: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): Microsoft sign-in gate (AuthGate) + header sign-out"
```

---

## Task 6: Specs (French)

**Files:** `SPEC_TECHNIQUE.md`, `SPEC_FONCTIONNELLE.md`

- [ ] **Step 1: Update SPEC_TECHNIQUE.md (French)**

Replace the Outlook-specific OAuth description with the Microsoft sign-in gate: routes `/auth/microsoft/login` + `/auth/microsoft/callback`; scopes `Calendars.Read Files.Read.All offline_access openid profile`; env `MICROSOFT_*`; config keys `microsoft.refresh_token`/`microsoft.access_token`/`microsoft.token_expires_at`/`microsoft.account`; `GraphTokenProvider` shared by Outlook + Excel connectors; GraphQL `session { authenticated account }` + `signOut`; admin consent granted tenant-wide; the frontend `AuthGate` startup gate.

- [ ] **Step 2: Update SPEC_FONCTIONNELLE.md (French)**

Describe: connexion Microsoft obligatoire au démarrage (porte d'authentification), un seul jeton couvrant Outlook + Excel/SharePoint, bouton « Se déconnecter », message « Sign-in required » si la session expire.

- [ ] **Step 3: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "docs(spec): document Microsoft sign-in gate and unified Graph token"
```

---

## Task 7: Final review + live verification

- [ ] **Step 1: Whole-branch build/test**

Run: `cd backend && cargo test -p domain -p application -p infrastructure -p api` (all pass) and `cd frontend && pnpm build` (clean). Note: the pre-existing `mcp` crate build break is unrelated and out of scope.

- [ ] **Step 2: Security re-check (auth/secrets changed)**

Dispatch `team/security` on the diff. Confirm: secret only in `.env`; refresh/access tokens never returned via GraphQL (`session` exposes only `authenticated`+`account`); `configuration` still redacts secret keys; CSRF state intact; CORS locked; `invalid_grant` clears the session.

- [ ] **Step 3: Live E2E**

1. Restart backend (`cd backend && cargo run -p api`) so it loads `MICROSOFT_*` env; start frontend (`pnpm dev`).
2. Open `http://localhost:3000` → expect the **"Sign in with Microsoft"** gate (app blocked).
3. Click it → sign in as **mbonenfant@witivio.com** (mailbox account) → frictionless (admin-consented) → back to app, gate gone, header shows the account.
4. Trigger Outlook sync → the 10 stale meetings clear and the live calendar (today→+`calendar_days`) populates.
5. (If SharePoint configured) trigger Excel sync → uses the same token, no separate auth.
6. Click **Sign out** → returns to the gate; a subsequent sync reports "Sign-in required".

- [ ] **Step 4: Confirm original goal**

"Restart from zero sync of meeting with Outlook" is met: after one app sign-in, the calendar reflects the live window with no manual token, and the same sign-in also powers Excel.

---

## Self-review notes

- **Spec coverage:** gate UX (Task 5), single-user session (no per-request auth — unchanged), one token both connectors (Tasks 2-3), admin consent (Task 1), rename (Task 2), `session`/`signOut` (Task 4), env/config rename (Tasks 1-2), specs (Task 6), security + E2E (Task 7). All spec sections mapped.
- **Single token fetch:** Task 3 Step 1 explicitly replaces the Task 2 Outlook-only block so there is exactly one `valid_access_token` call feeding both clients (no double refresh).
- **Type/key consistency:** `GraphTokenProvider::valid_access_token(user_id) -> Result<String, AppError>` (Tasks 2,3); config keys `microsoft.{access_token,refresh_token,token_expires_at,account}` identical across Tasks 2,3,4; routes `/auth/microsoft/{login,callback}` (Tasks 1,2,5); GraphQL `session`/`SessionGql`/`signOut` (Tasks 4,5).
- **Out of scope:** per-request backend auth/multi-user; token-at-rest encryption; the broken `mcp` crate.
```
