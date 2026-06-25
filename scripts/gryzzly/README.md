# Gryzzly Keyless Catalog Refresh

## Why keyless?

No Gryzzly API key is available. Instead of an automated backend sync, this tooling performs a **manual, repeatable snapshot**: the user's own browser session token (already present after login) is borrowed in-page to pull the catalog, which is then downloaded as a local JSON file and imported into the cockpit DB.

This is a deliberate trade-off: a few manual steps every time instead of a persistent credential.

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
Authentication: `Authorization: Bearer <token>` header  
All calls: `POST` with `Content-Type: application/json`

### List projects

```
POST /view/projects.list
Body: {"filter":"","range":"","search":"","limit":1000}
Response: {payload: [{id, name, status, customer_name, ...}, ...]}
```

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
