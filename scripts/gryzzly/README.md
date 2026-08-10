# Gryzzly Keyless Catalog Refresh — Fallback Path

## When to use this

**Normally you don't.** The backend sync now does this automatically: run
`aplan sync --source gryzzly`. It reads the same session cookie from your local Chromium profile,
decrypts it, and pulls the catalog over the internal API. See `SPEC_TECHNIQUE.md` §10.6.

This tooling is the **fallback** for when that cookie read breaks:

- a browser upgrade changes Chromium's on-disk cookie layout or encryption tag,
- the OS keyring is unavailable or locked, so the AES key cannot be recovered,
- the API runs somewhere with no browser profile at all.

The other escape hatch, which needs no scripts, is to paste a token into configuration:
`aplan config set gryzzly.token "User <token>"`. Use the bookmarklet trick below to obtain it.

## Why keyless?

No Gryzzly API key exists — the product issues none. The only credential is the `remember_token`
session cookie from the Microsoft SSO login, valid for a fixed 7 days. This script borrows it
in-page to pull the catalog, downloads it as a local JSON file, and imports it into the cockpit DB.

---

## 2-Step Refresh

### Step 1 — Export from Gryzzly (browser)

1. Go to <https://app.gryzzly.io> and log in.
2. Open DevTools → **Console** tab.
3. Paste the entire contents of `scripts/gryzzly/export-catalog.console.js` and press Enter.
4. Click **any project** once (or refresh the page once).
5. `gryzzly_catalog.json` downloads automatically to your browser's downloads folder.

### Step 2 — Import into the cockpit DB

> ⚠️ `import_catalog.py` **REPLACES all `gryzzly_tasks` rows** on each run. It will refuse to proceed if the export file is empty (e.g. a truncated download), so your catalog is never accidentally wiped.

```bash
python3 scripts/gryzzly/import_catalog.py
# optional explicit paths:
python3 scripts/gryzzly/import_catalog.py ~/Téléchargements/gryzzly_catalog.json backend/aggregated_plan.db 00000000-0000-0000-0000-000000000001
```

After import, reload the cockpit frontend. The backend re-reads the DB on each request, so no backend restart is needed.

---

## Real API Contract (reverse-engineered)

Base URL: `https://api.gryzzly.io` (no `/v1` prefix)  
Authentication: `Authorization: User <session-token>` header (**not** `Bearer`)  
All calls: `POST` with `Content-Type: application/json` — reads included; the API is RPC-style  
Envelope: `{ok, payload}`, plus `cursor` on list methods

### List projects

```
POST /view/projects.list
Body: {"filter":"","range":"","search":"","limit":500}
Response: {ok: true, cursor: null|"<uuid>", payload: [{id, name, status, customer_name, ...}, ...]}
```

`limit` **caps at 500** — 1000 is rejected with
`{"ok":false,"errors":["decoding: invalid_argument: limit (out of range, max=500)"]}`. This script
sent 1000 and was silently broken until it was fixed.

`limit` is a batch size applied *before* filtering, so pages come back shorter than requested and a
short or empty page does **not** mean the end. Paginate with the `cursor` parameter (echo back the
value received) and stop only when `cursor` is null. This script does not paginate: with 37 projects
one page suffices, but it will truncate if the team ever exceeds 500 pre-filter rows.

A project is active when `status == "active"` (observed values: `active`, `done`) and `deleted_at`
is null. There is no `archived` field.

### Get project tasks (including nested)

```
POST /expandedProjectMetrics.get
Body: {"project_id": "<uuid>"}
Response: {payload: {tasks: [{id, name, project_id, completed_at, deleted_at, is_container, parent_id, tasks: [...nested...]}, ...]}}
```

The export script flattens all nested `tasks` arrays recursively. A task is considered **active** when both `completed_at` and `deleted_at` are null/absent.

### Exported JSON row shape

Each element in `gryzzly_catalog.json` is a 6-element array:

```
[gryzzly_task_id, name, gryzzly_project_id, project_name, customer_name, active(0|1)]
```

---

## DB Table

```sql
gryzzly_tasks(
  id TEXT PRIMARY KEY,
  user_id TEXT,
  gryzzly_task_id TEXT,
  name TEXT,
  gryzzly_project_id TEXT,
  project_name TEXT,
  customer_name TEXT,
  is_active INTEGER,
  last_synced_at TEXT,
  UNIQUE(user_id, gryzzly_task_id)
)
```

`import_catalog.py` **replaces** all rows for the given `user_id` on each run (DELETE then INSERT).

---

## Privacy

The session token is intercepted **in-page only** by the console snippet: it is captured by patching `XMLHttpRequest.prototype.setRequestHeader` inside the browser tab's JavaScript sandbox. The token is:

- never written to disk or localStorage
- never sent to any server other than `api.gryzzly.io` (same destination the app uses itself)
- gone as soon as the tab is closed or refreshed

Only the catalog data (project and task names) leaves the browser, as the downloaded `gryzzly_catalog.json` file.
