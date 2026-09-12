# CLAUDE.md

Guide for AI assistants working on the Aggregated Plan codebase.

## Project Overview

A personal Tech Lead cockpit that aggregates Jira tasks, Outlook meetings, and Excel/SharePoint data into a unified planning view. Features include priority matrix (Eisenhower), workload visualization, activity tracking with half-day granularity, automatic deduplication, and real-time alerts. Currently in MVP phase with SQLite storage.

## Repository Structure

```
aggregated_plan/
├── backend/                      # Rust workspace (Cargo)
│   ├── Cargo.toml                # Workspace root
│   ├── crates/
│   │   ├── domain/               # Pure business logic, zero I/O
│   │   ├── application/          # Use cases, repository traits, service traits
│   │   ├── infrastructure/       # SQLite repos, HTTP connectors, sync engine
│   │   └── api/                  # Axum server + async-graphql resolvers
│   └── .env.example
├── frontend/                     # React 18 + Vite (port 3000)
├── migrations/
│   └── sqlite/                   # SQLite migration files
│       └── 001_initial.sql
├── docs/
│   └── plans/                    # Implementation plans
├── SPEC_FONCTIONNELLE.md         # Functional specification (French)
├── SPEC_TECHNIQUE.md             # Technical specification
└── CLAUDE.md                     # This file
```

### Backend DDD Layer Separation

```
backend/crates/
├── domain/          # Pure types, business rules. NO external deps (except chrono/serde/uuid/thiserror)
├── application/     # Repository traits, service traits, use case functions. Depends on domain only.
├── infrastructure/  # SQLite repos (sqlx), HTTP connectors (reqwest), sync/dedup engines
└── api/             # Axum routes, async-graphql schema, middleware. Depends on all layers.
```

## Quick Reference Commands

```bash
# Backend (Rust)
cd backend && cargo build                    # Build all crates
cd backend && cargo test                     # Run all backend tests
cd backend && cargo test -p domain           # Domain tests only (52 tests)
cd backend && cargo test -p infrastructure   # Infrastructure tests only (50 tests)
cd backend && cargo check                    # Type-check without building
cd backend && cargo run -p api               # Start API server (port 3001)
cd backend && cargo clippy                   # Lint

# Frontend (TypeScript/React) — not yet set up
cd frontend && pnpm install                  # Install dependencies
cd frontend && pnpm dev                      # Start dev server (port 3000)
cd frontend && pnpm test                     # Run tests
cd frontend && pnpm build                    # Production build
```

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Backend language | Rust (stable) |
| HTTP framework | Axum 0.7 |
| GraphQL | async-graphql 7 (queries, mutations, SSE subscriptions) |
| Database | SQLite via sqlx 0.8 (compile-time unchecked, runtime queries) |
| Async runtime | Tokio 1 |
| HTTP client | reqwest 0.12 |
| Frontend language | TypeScript 5.3+ (strict) |
| Frontend framework | React 18, Vite 5 |
| GraphQL client | urql 4, graphql-sse |
| UI components | shadcn/ui, Tailwind CSS 3 |
| Charts | Recharts 2 |
| Drag & drop | @dnd-kit |
| Testing (backend) | Rust built-in `#[test]` + tokio::test |
| Testing (frontend) | Vitest, React Testing Library, Playwright (E2E) |

## Mandatory Coding Conventions

### DDD Layer Rules (strict)

- **Domain** (`crates/domain/`): Pure business logic. Zero I/O. Only depends on chrono, serde, uuid, thiserror.
- **Application** (`crates/application/`): Defines repository and service traits. Use case functions. Depends only on domain.
- **Infrastructure** (`crates/infrastructure/`): Implements traits with real I/O (SQLite, HTTP). Depends on domain + application.
- **API** (`crates/api/`): Axum server, GraphQL resolvers, middleware. Depends on all layers.

### Rust Conventions

- Use `struct` with `impl` blocks. No OOP inheritance.
- Factory pattern: `StructName::new(...)` associated functions
- Repository pattern: traits in application, implementations in infrastructure
- Error handling: `thiserror` for error enums, `Result<T, E>` everywhere, no `.unwrap()` in production
- Domain functions return `DomainResult<T>` (alias for `Result<T, DomainError>`)
- Use `async_trait` for async trait definitions
- Map `sqlx::Error` → `RepositoryError::Database(e.to_string())`

### TypeScript/Frontend Conventions

- Strict TypeScript (all strict flags enabled)
- Functional components with hooks
- urql for GraphQL queries/mutations
- shadcn/ui components (New York variant)
- Tailwind CSS for styling
- `const` over `let`, never `var`

### Spec Maintenance

Whenever code changes affect documented behaviour (API endpoints, domain rules, UI features, data
model, config keys), update **SPEC_FONCTIONNELLE.md** and/or **SPEC_TECHNIQUE.md** in the same
commit. Specifications are written in French.

### Test-Driven Development

Write tests BEFORE production code. Follow Red → Green → Refactor cycle.

Backend tests are inline with `#[cfg(test)] mod tests`. Integration tests use in-memory SQLite (`sqlite::memory:`).

## Naming Conventions

| Entity | Backend (Rust) | Frontend (TypeScript) |
|--------|---------------|----------------------|
| Types/Structs | PascalCase | PascalCase |
| Functions | snake_case | camelCase |
| Constants | UPPER_SNAKE_CASE | UPPER_SNAKE_CASE |
| Files | snake_case (`task_repo.rs`) | kebab-case (`task-list.tsx`) |
| Modules | snake_case | kebab-case |

## GraphQL API

The backend exposes a single GraphQL endpoint:
- `POST /graphql` — queries and mutations
- `GET /graphql/sse` — SSE subscriptions

Key queries: `tasks`, `task`, `projects`, `dashboard`, `priorityMatrix`, `workload`, `alerts`
Key mutations: `createTask`, `updateTask`, `deleteTask`, `updatePriority`, `startActivity`, `stopActivity`, `triggerSync`

## Database

SQLite with migrations at `migrations/sqlite/`. All IDs are UUID strings (`TEXT`). Dates stored as ISO 8601 `TEXT`. Enums as lowercase `TEXT`. Booleans as `INTEGER` (0/1).

27 tables: users, projects, tasks, task_tags, task_links, meetings, activity_slots, alerts, tags, sync_status, configuration, worklog_entries, task_recurrences, task_recurrence_tags, gryzzly_tasks, timesheet_drafts, timesheet_draft_lines, timesheet_quarter_shares, signal_project_mappings, memories, memory_stakeholders, memories_fts, sessions, break_rules, break_events, claude_usage_requests, claude_usage_files.

Timesheet quarter arbitration (migration `018`): the day is four two-hour quarters cut from
the configured windows, and `timesheet_quarter_shares` holds one row per (draft, quarter,
lane) — a **billing decision**, hence a table rather than JSON. Evidence becomes overlapping
per-task *presence lanes* (`domain/src/rules/presence.rs`): each worklog entry casts a
back-shadow of at most `MAX_CONTINUATION_GAP_MINUTES`, clipped at its own lane's previous
entry and never at another lane's. Each quarter's hours are apportioned across the lanes
present in it by presence weight (`domain/src/rules/quarters.rs`), and `is_pinned` marks a
share the user set by hand, which a reconstruct preserves. The day totals its quarters, not
`workday.daily_target_hours`.

Sessions (migration `014`): `sessions` is one row per Claude Code session, keyed by the
harness's `CLAUDE_CODE_SESSION_ID`, so several concurrent sessions can log against different
tasks. The global `aplan.active_task_id` pointer keeps its own meaning — the human, working by
hand — and the two never merge. `worklog_entries.session_id` and `activity_slots.session_id`
carry authorship (NULL = the human), and `activity_slots.source` (`worklog` | `manual`, NULL
read as `manual`) marks which slots the worklog projection owns and may therefore rebuild.

Claude usage index (migration `024`): `claude_usage_requests` is **one row per API
request**, keyed by `request_id`, and that key is the design. A single call writes
one `assistant` line per content block — thinking, text, tool_use — and **each
repeats the identical `usage` object**: 56 151 lines for 28 675 requests on the real
corpus, so summing lines inflates the burn by 1.89x. The primary key also buys
idempotence, which is what makes `claude_usage_files` (path, size, mtime, offset)
a pure speed cache: a lost cursor costs one rescan (5.5 s over 663 files; 42 ms once
the cursor is warm), never a wrong total. A file that has **shrunk below its
recorded offset** was compacted and is re-read from zero. "Consumed" is
`input + output + cache_creation` — `cache_read` is 35x all of it together and would
turn the gauge into a cache-hit meter, and `thinking_tokens` is a *part of*
`output_tokens`, never a cost beside it. The ceiling is unmeasurable (no public API
exposes the subscription quota) and lives in `configuration` under
`aplan.claude.declared_ceiling_tokens`; until it is set the HUD draws **no gauge at
all**. Anything not `type: "assistant"`, and the `<synthetic>` placeholder, is
excluded. The tree is nested — subagent transcripts live in `<session>/subagents/`
and deeper — so the walk is recursive; 47% of requests are sidechains.

Semantic memory (migration `012`): `memories` is bi-temporal (`occurred_at` / `invalidated_at` / `superseded_by`) with stakeholders in the junction table `memory_stakeholders`, and `memories_fts` is a **standalone** FTS5 index (no `content=`, no triggers) that the repository writes in the same transaction as the memory row.

Break routine (migration `019`): `break_rules` holds the superposed cadences (interval or daily)
and `break_events` the trace of every due slot. The engine (`domain/src/rules/breaks.rs`) is
wall-clock anchored on the `workday.*` windows, never on the last fire — a break that was missed,
snoozed or absorbed does not shift the grid. `break_rules.priority` exists because the built-in
cadences overlap by construction (20/30/60 min all coincide at minute 60): the engine fires at
most one notification per tick, the highest priority, and marks the rest `absorbed`. The
adherence rate (`taken / seen`) excludes `absorbed` and `expired` from both sides — they never
reached a screen, so counting them would drown a real signal in scheduling noise.

Break sessions (migration `021`): pressing the notification's button no longer records a
`taken` — it **opens a session**. `break_events.started_at` / `ends_at` are stamped, the
outcome stays `pending`, and the tick that fired the break holds it open for the whole pause,
which is why no break can ring during a break. `ends_at` is frozen at the press (not
recomputed from the rule, so retuning a duration mid-pause cannot lengthen it) and anchored
on the press rather than on the tick's `now`, because `notify-send --action` implies `--wait`
and the tick's `now` can be minutes old by then. `taken` is written only at the deadline;
cutting the pause short from the HUD writes `abandoned`, which **does** count against
adherence — it was seen and answered. `endBreak` is a compare-and-swap in SQL
(`WHERE outcome='pending' AND started_at IS NOT NULL`), so the tick's unconditional `taken`
wins the race by construction rather than by timing. Orphan recovery deliberately claims only
sessions whose `ends_at` has passed, closing them as `taken`: a live session belongs to
whoever is serving it, and a blanket recovery let a second API process abandon a running
pause, turned an NTP step backwards into a false abandon, and docked the adherence rate on
every restart. The visual lives in the Tauri HUD, shown through `SurfaceController`
(`application/services`) → `aplan-hud-toggle show|hide`; a surface that will not come up is
logged, never fatal — a pause without a screen is still a pause.

Offline capture idempotency (migration `023`): `tasks.client_request_id` carries a key the
mobile PWA generates **at input time, not at send time** — generating it at send would give two
sends of one entry two keys, which is the duplicate the column exists to prevent. The unique
index is **partial** (`WHERE client_request_id IS NOT NULL`) because the desktop path sends
nothing and leaves the column `NULL`. `create_personal_task` short-circuits on a known key and
returns the existing task, so a replay is an observable no-op rather than an error the client
must interpret. The index stays honest because `save` upserts with `ON CONFLICT(id) DO UPDATE`,
never `INSERT OR REPLACE` — a key conflict raises, it does not delete and re-insert.

Network exposure: the API bind stays `127.0.0.1:3001`. Remote access goes through
`tailscale serve` (**never `funnel`**, which publishes to the public internet), and
`APLAN_STATIC_DIR` makes the API serve `frontend/dist` so the page and `/graphql` share one
origin. `graphql_handler` authenticates nothing per request, so reachability still equals
authority — that is why the bind must not move. See `SPEC_TECHNIQUE.md` § 25 and
`scripts/aplan-serve-tailnet`.

## Key Domain Concepts

- **Half-day granularity**: Activity tracking uses morning (08:00-12:00) and afternoon (13:00-17:00) slots
- **Priority matrix**: Eisenhower quadrant based on urgency (1-4) × impact (1-4)
- **Urgency calculation**: Auto-computed from deadline proximity (R10-R14), manual override possible (R15)
- **Workload detection**: Overload alerts when planned hours + meeting hours > capacity (R16)
- **Deduplication**: Jira key matching (R08) + similarity scoring (R09) with 0.7 threshold
- **External integrations**: Read-only sync from Jira REST API, Microsoft Graph (Outlook + Excel/SharePoint)
- **Multi-user ready**: All tables include `user_id`, auth middleware injects default user locally

## Common Gotchas

- **Rebuild the API with `--release` before publishing it on the tailnet.** The
  `/graphql/playground` route is mounted only under `cfg!(debug_assertions)`; a debug binary
  behind the tunnel would hand every tailnet device a schema console on an API that
  authenticates nothing. `scripts/aplan-serve-tailnet` probes that route and refuses to publish
  if it answers.
- The frontend has **no GraphQL codegen**. Queries are template literals exported from
  `frontend/src/graphql/queries/*.ts`; the `.graphql` files in that directory are dead
  scaffolding (`tasks.graphql` has unbalanced braces). Adding a schema field needs no frontend
  regeneration.
- `frontend/src/presentation/` is dead scaffolding from the initial commit and its
  `app.test.tsx` has always failed (it imports a `@application/index` alias that never
  existed). One failing suite in `pnpm test` is the expected baseline, not a regression.
- The API router serves static files via `fallback_service`, so `/graphql` keeps priority and
  the CSRF guard is never bypassed. The SPA fallback uses `ServeDir::fallback`, **not**
  `not_found_service`, which forces a 404 status via `SetStatus` and would break a reload on
  `/m/new`.
- `sqlx::migrate!` macro path is relative to the crate's `Cargo.toml`, not the workspace root
- Infrastructure repos use runtime queries (`sqlx::query`), not compile-time checked (`sqlx::query!`)
- The `participants` field in meetings and `related_items` in alerts are JSON-serialized `TEXT` columns
- Task tags live in a junction table `task_tags`, not as a column on the tasks table
- Specifications are written in French; code and comments should be in English
- Backend serves on port 3001, frontend on port 3000
- **Never exercise the GraphQL API against `http://127.0.0.1:3001` to test anything.**
  That port serves the real `aggregated_plan.db`. Two recurrence templates created by
  live test calls left 42 dead tasks behind, piling up as overdue cards for four
  months, and a past recurrence occurrence was removable by no path at all. To
  exercise the API by hand, run an instance on a throwaway database:
  `DATABASE_URL=sqlite:///tmp/aplan-dev.db cargo run -p api`. The listening port is
  **hardcoded** to 3001 (`api/src/main.rs`, no env override), so that instance cannot
  coexist with the installed one — stop the service first
  (`systemctl --user stop aplan-api.service`), work against the throwaway database,
  then restart it. The same applies to the Playwright suite:
  `frontend/e2e/` reads `APLAN_E2E_GRAPHQL_URL` and skips entirely when it is unset,
  precisely so a bare `pnpm test:e2e` cannot write into the cockpit's own data.
  Note that `frontend/playwright.config.ts` still starts its backend with
  `cargo run -p api` and `reuseExistingServer: !CI`, so a UI-driven write from
  another spec can still reach the real database — set the variable and point it at a
  throwaway instance.
- `cargo run -p api -- export-schema` builds the database pool **before** printing the
  SDL, so regenerating `crates/cli/graphql/schema.graphql` applies any pending
  migration to the real database. It prints to stdout — redirect it explicitly.
