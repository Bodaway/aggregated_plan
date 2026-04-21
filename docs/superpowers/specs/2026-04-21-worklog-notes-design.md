# Worklog notes — design

**Status:** approved 2026-04-21
**Author:** brainstorming session with user

## 1. Problem

Today a task has a single `notes: Option<String>` markdown blob. The activity timer appends quick notes into that blob via `appendTaskNotes`. There is no way to:

- see what you logged **when** (no per-entry timestamp),
- review a day's logs across tasks, or
- review one task's log in chronological order.

We want a first-class, timestamped, task-scoped log, plus a global "Worklog" tab with day and task filters.

## 2. Decisions (from brainstorming)

| # | Decision |
|---|----------|
| D1 | New `worklog_entries` table. `tasks.notes` stays untouched. |
| D2 | Entries are journal-style: timestamp auto = now, body editable, entry deletable. Secondary "edit timestamp" action exists but is not on the primary path. |
| D3 | Entries are independent of `ActivitySlot` (no FK). Half-day grouping is derivable from `logged_at` if ever needed. |
| D4 | Worklog tab layout: timeline grouped by day, newest first. |
| D5 | TaskEditSheet shows worklog as an inline section below the `notes` textarea (no tab switching). |
| D6 | Entry body is markdown, consistent with `tasks.notes`. |
| D7 | The activity-timer stop flow writes a `worklog_entry` instead of calling `appendTaskNotes`. The `appendTaskNotes` mutation stays in the API for backward compatibility but is no longer called from the UI. |

## 3. Data model

New SQLite table, migration `migrations/sqlite/006_create_worklog_entries.sql`:

```sql
CREATE TABLE worklog_entries (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    body       TEXT NOT NULL,
    logged_at  TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_worklog_entries_user_logged_at ON worklog_entries(user_id, logged_at DESC);
CREATE INDEX idx_worklog_entries_task_logged_at ON worklog_entries(task_id, logged_at DESC);
```

- All IDs are UUID strings (project convention).
- Timestamps stored as ISO 8601 UTC text.
- `ON DELETE CASCADE` on task: deleting a task removes its entries.
- `body` is non-empty (validated in the domain layer; DB has no `CHECK` to keep migration simple).

Domain type — `backend/crates/domain/src/types/worklog.rs`:

```rust
pub struct WorklogEntry {
    pub id: Uuid,
    pub user_id: Uuid,
    pub task_id: Uuid,
    pub body: String,
    pub logged_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorklogEntry {
    pub fn new(user_id: Uuid, task_id: Uuid, body: String, logged_at: DateTime<Utc>) -> DomainResult<Self>;
}
```

Validation in `new`:

- `body.trim()` must be non-empty → `DomainError::Validation("worklog body cannot be empty")`.
- `body.len()` ≤ 10_000 chars → `DomainError::Validation("worklog body too long")`.

## 4. Application layer

New repository trait — `application/src/repositories/worklog_repository.rs`:

```rust
#[async_trait]
pub trait WorklogRepository: Send + Sync {
    async fn create(&self, entry: &WorklogEntry) -> RepoResult<()>;
    async fn update(&self, entry: &WorklogEntry) -> RepoResult<()>;
    async fn delete(&self, id: Uuid, user_id: Uuid) -> RepoResult<bool>;
    async fn find_by_id(&self, id: Uuid, user_id: Uuid) -> RepoResult<Option<WorklogEntry>>;
    async fn list(&self, user_id: Uuid, filter: WorklogFilter) -> RepoResult<Vec<WorklogEntry>>;
}

pub struct WorklogFilter {
    pub task_ids: Option<Vec<Uuid>>, // None = all tasks
    pub from: Option<DateTime<Utc>>, // inclusive lower bound on logged_at
    pub to: Option<DateTime<Utc>>,   // exclusive upper bound on logged_at
    pub limit: u32,                  // default 200, max 1000
    pub offset: u32,                 // default 0
}
```

Use cases — `application/src/worklog.rs`:

- `add_worklog_entry(repo, task_repo, user_id, task_id, body, logged_at_override) -> DomainResult<WorklogEntry>` — verifies the task belongs to the user, builds `WorklogEntry`, persists.
- `update_worklog_entry(repo, user_id, id, body?, logged_at?) -> DomainResult<WorklogEntry>` — partial update; rejects if the entry is not owned by `user_id`.
- `delete_worklog_entry(repo, user_id, id) -> DomainResult<bool>`.
- `list_worklog_entries(repo, user_id, filter) -> DomainResult<Vec<WorklogEntry>>`.

`logged_at` defaults to `Utc::now()` when the caller passes `None`.

## 5. Infrastructure layer

- `infrastructure/src/persistence/sqlite/worklog_repo.rs` implements `WorklogRepository` using runtime `sqlx::query` (project convention: not the compile-time macro).
- Rows ordered by `logged_at DESC, created_at DESC` (tiebreak) in `list`.
- `create` / `update` set `updated_at = now`.

## 6. GraphQL API

New types — `api/src/graphql/types/worklog_entry.rs`:

```graphql
type WorklogEntry {
  id: ID!
  taskId: ID!
  task: Task!              # resolver hydrates via TaskRepository
  body: String!
  loggedAt: DateTime!
  createdAt: DateTime!
  updatedAt: DateTime!
}

input WorklogEntryFilter {
  taskIds: [ID!]
  from: DateTime
  to: DateTime
  limit: Int  = 200
  offset: Int = 0
}
```

Queries on `QueryRoot`:

- `worklogEntries(filter: WorklogEntryFilter): [WorklogEntry!]!` — uses the authenticated `user_id`.

Mutations on `MutationRoot`:

- `addWorklogEntry(taskId: ID!, body: String!, loggedAt: DateTime): WorklogEntry!`
- `updateWorklogEntry(id: ID!, body: String, loggedAt: DateTime): WorklogEntry!`
- `deleteWorklogEntry(id: ID!): Boolean!`

`appendTaskNotes` stays registered (no breaking change) but is no longer invoked by the frontend.

## 7. Frontend — inline section in TaskEditSheet

Location: `frontend/src/components/task/TaskEditSheet.tsx`, new `<TaskWorklogSection taskId={...} />` rendered directly below the existing `notes` textarea.

Layout:

```
┌─ Worklog ─────────────────────────────────────────────┐
│  [ markdown textarea ............................. ]  │
│  Ctrl+Enter to log           [ Log entry ]            │
├────────────────────────────────────────────────────────┤
│  21 Apr 16:42   Wrote the design doc                  │  ⋮
│  21 Apr 14:10   Started on repo survey                 │  ⋮
│  20 Apr 11:05   Spec locked after call with team       │  ⋮
└────────────────────────────────────────────────────────┘
```

- Markdown bodies are rendered in the list (same renderer used elsewhere in the app).
- Hover/focus reveals a kebab `⋮` per entry: **Edit**, **Delete**, **Edit timestamp…** (last one opens a small popover with a datetime input — secondary path, not a button).
- Submit: plain Enter inserts newline; `Ctrl/Cmd+Enter` submits; empty bodies disabled.
- List fetches last 50 entries for this task; "Show older" loads more.
- Optimistic add: entry appears instantly; on error, it's rolled back with a toast.

## 8. Frontend — new Worklog tab

Route: `/worklog`. Sidebar entry between **Activity** and **Deduplication** (icon: `BookText` or similar lucide icon).

Page composition:

- **Top bar** — date range presets (chips): `Today`, `Last 7 days`, `This week`, `This month`, `Custom…` (opens calendar popover). Plus a task/project multi-select (reuses the same combobox as search bar filters).
- **Body** — timeline grouped by day, newest day first:
  - Day header: e.g. `Tuesday 21 Apr — 3 entries`.
  - Within a day: entries newest-first. Each entry card: `HH:mm` timestamp · task chip (click → opens that task in `TaskEditSheet`) · rendered markdown · kebab (Edit / Delete / Edit timestamp).
- **Default range:** `Last 7 days` to avoid unbounded queries.
- **Empty state:** "No entries for this range."
- **Pagination:** backend-enforced `limit` (default 200, max 1000); UI loads more on scroll.

## 9. Activity-timer stop flow (backward compat)

Current: frontend `stopActivity` mutation + optional `appendTaskNotes`.

New: `stopActivity` mutation + optional `addWorklogEntry(taskId, body, loggedAt: slot.end_time)`.

- Backend `append_task_notes` resolver untouched — still callable, just unused from the UI.
- One frontend file changes: the component/hook that runs the stop flow.

## 10. Spec updates

Per project convention, update in the same PR:

- **`SPEC_FONCTIONNELLE.md`** — add feature rules under a new "Journal de bord" section:
  - R-WL-01: une entrée de worklog est toujours attachée à une tâche.
  - R-WL-02: horodatage automatique à la création; modifiable via action secondaire.
  - R-WL-03: corps en markdown, non vide, max 10 000 caractères.
  - R-WL-04: vue Worklog filtrable par plage de dates et par tâche, regroupée par jour.
  - R-WL-05: suppression d'une tâche supprime ses entrées de worklog.
  - R-WL-06: l'arrêt du timer d'activité avec note crée une entrée de worklog (remplace l'ancien comportement d'ajout au champ `notes`).
- **`SPEC_TECHNIQUE.md`** — new `worklog_entries` table, GraphQL types/queries/mutations, index list.

## 11. Testing

Backend:

- **Domain:** `WorklogEntry::new` validation (empty body, whitespace-only, oversize).
- **Application:** use-case tests with an in-memory fake repo (ownership enforcement, filter behavior).
- **Infrastructure:** sqlx tests against `sqlite::memory:`, including:
  - round-trip create → find_by_id,
  - list ordering (`logged_at DESC`),
  - filter combinations (date range, task set, limit/offset),
  - cascade on task delete.
- **API:** async-graphql resolver tests for add/update/delete/list, including auth-isolation.

Frontend:

- **Component tests (Vitest + RTL):**
  - `TaskWorklogSection` renders entries, Ctrl+Enter submits, optimistic add + rollback on error, kebab actions.
  - `WorklogPage` filters, empty state, day grouping.
- **E2E (Playwright):** open a task → add entry → navigate to `/worklog` → entry appears under today's header; filter to yesterday → empty state.

## 12. Out of scope (explicit)

- Standup-style orphan entries (no task).
- Attachments / images.
- FK to `ActivitySlot`.
- Migration of existing `tasks.notes` content into entries.
- Real-time SSE push for new entries (refetch on mutation is sufficient for a single-user cockpit).
