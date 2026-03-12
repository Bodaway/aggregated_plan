# Code Review Report — Aggregated Plan

**Date:** 2026-03-12
**Scope:** Full codebase review (backend, frontend, migrations, config, specs)
**Methodology:** Layer-by-layer static analysis covering security, correctness, design, and maintainability

---

## Executive Summary

The codebase follows a clean DDD architecture with proper layer separation. However, the review uncovered **critical security vulnerabilities** (authentication bypass, CORS misconfiguration, SSRF), **data integrity bugs** (SQL CHECK constraint mismatch, missing transactions), and **frontend reliability issues** (silent error swallowing, race conditions). The domain layer is well-isolated but lacks input validation. The infrastructure layer has no transaction safety. The API layer exposes the application with effectively no security controls.

**Finding totals by severity:**

| Severity | Backend | Frontend | Infra/DB | Total |
|----------|---------|----------|----------|-------|
| Critical | 2 | 3 | 1 | **6** |
| High | 6 | 6 | 2 | **14** |
| Medium | 10 | 9 | 8 | **27** |
| Low | 5 | 8 | 6 | **19** |

---

## 1. CRITICAL Findings

### SEC-01: Authentication middleware not wired — all endpoints unauthenticated
**Location:** `backend/crates/api/src/main.rs:60-67`, `schema.rs:41-42,58`

The `auth` middleware module exists but is never applied to any route. Instead, a hardcoded default user UUID is injected into GraphQL context data unconditionally. **Any network-accessible client has full access to all data and mutations.**

**Impact:** Complete authentication bypass.
**Fix:** Wire `AuthMiddleware` into the Axum router and validate tokens on every request.

---

### SEC-02: CORS fully permissive — accepts any origin
**Location:** `backend/crates/api/src/main.rs:63`

```rust
CorsLayer::permissive()
```

Allows any origin with any method and any headers. Combined with SEC-01, any website can make authenticated API calls.

**Impact:** Cross-origin data theft and CSRF.
**Fix:** Restrict `allowed_origins` to `http://localhost:3000` (or configurable list).

---

### SEC-03: `Source::Outlook` blocked by SQL CHECK constraints
**Location:** `migrations/sqlite/001_initial.sql` — `tasks.source` and `projects.source` CHECK constraints

The domain defines `Source::Outlook` and the infrastructure maps it to `"outlook"`, but the database CHECK constraints only allow `('jira', 'excel', 'obsidian', 'personal')`. Any Outlook sync INSERT will fail at runtime with a constraint violation.

**Impact:** Outlook sync is completely broken at the database level.
**Fix:** Add `'outlook'` to both CHECK constraints via a new migration.

---

### SEC-04: API tokens stored and transmitted in plaintext
**Location:** `frontend/src/pages/SettingsPage.tsx`, `backend/crates/infrastructure/`

Jira API tokens and Microsoft Graph access tokens are sent via plain GraphQL mutations, stored as plain text in the SQLite `configuration` table, and exposed back via the `configuration` GraphQL query.

**Impact:** Credential exposure.
**Fix:** Encrypt secrets at rest, never return them in queries, enforce HTTPS.

---

### SEC-05: Unsafe `undefined as T` return in API client
**Location:** `frontend/src/infrastructure/api-client.ts:43`

On non-OK responses for void-like calls, the function returns `undefined as T`. Callers receive a silently wrong value with no type error at compile time.

**Impact:** Silent data corruption and downstream crashes.
**Fix:** Throw an error on non-OK responses instead of returning `undefined`.

---

### SEC-06: No React error boundary in the application
**Location:** `frontend/src/main.tsx`, `frontend/src/App.tsx`

No error boundary wraps the component tree. Any unhandled rendering exception crashes the entire application to a white screen.

**Impact:** Complete UI failure with no recovery path.
**Fix:** Add an `ErrorBoundary` component at the app root.

---

## 2. HIGH Findings

### BUG-01: Overload alert compares daily meeting hours against weekly capacity
**Location:** `backend/crates/application/src/use_cases/dashboard.rs:77-120`

The dashboard fetches daily meeting hours but compares them against `weekly_capacity_hours`. Since daily values are always much smaller than weekly capacity, overload alerts effectively never trigger.

**Fix:** Either aggregate to weekly totals or compare against daily capacity (`weekly / working_days`).

---

### BUG-02: `check_deadline_alerts` fires on completed/dismissed tasks
**Location:** `backend/crates/domain/src/rules/alerts.rs:35-72`

Tasks with status `Done` or tracking state `Dismissed` still generate deadline alerts, creating noise.

**Fix:** Filter out completed/dismissed tasks before generating alerts.

---

### BUG-03: Sync status stuck at "Syncing" on failure
**Location:** `backend/crates/application/src/use_cases/sync.rs`

For Outlook and Excel sources, if the sync fails after the status is set to `Syncing`, no error handler updates the status. It remains permanently stuck.

**Fix:** Use a `finally`-style pattern to always update sync status on completion.

---

### BUG-04: Pagination cursor not reset when filter changes
**Location:** `frontend/src/hooks/use-alerts.ts:60`, `frontend/src/pages/AlertsPage.tsx`

The `after` cursor persists when switching between alert filters, producing missing or incorrect results.

**Fix:** Call `resetPagination()` in the filter change handler.

---

### BUG-05: Task edit form resets while user is editing
**Location:** `frontend/src/components/task/TaskEditSheet.tsx:64`

`useEffect` depends on `[task]` (object reference). Every urql background re-fetch creates a new reference, resetting all local form fields and discarding unsaved edits.

**Fix:** Compare by task ID or use a stable reference (e.g., `task.id + task.updated_at`).

---

### BUG-06: Race condition in optimistic drag-and-drop state
**Location:** `frontend/src/pages/DashboardPage.tsx:436-498`

A single `isMutatingRef` boolean guards all concurrent mutations. Rapid successive drags cause visual reverts when the first mutation completes and clears the lock.

**Fix:** Track in-flight mutations per task ID rather than a global boolean.

---

### SEC-07: GraphiQL playground unconditionally exposed
**Location:** `backend/crates/api/src/main.rs:62`, `schema.rs:71-77`

The interactive GraphQL playground is always served, including in production builds. Combined with SEC-01, this gives attackers a convenient interface.

**Fix:** Gate behind a `DEV` or `DEBUG` environment variable.

---

### SEC-08: Missing authorization on all entity mutations (IDOR)
**Location:** `backend/crates/application/src/use_cases/` — `task_management.rs`, `configuration.rs`

`update_task`, `delete_task`, `complete_task`, `resolve_alert`, `delete_tag`, etc. accept an entity ID without verifying it belongs to the calling user. Any user can modify any entity if they know the UUID.

**Fix:** Add `user_id` checks in every use case that retrieves an entity by ID.

---

### SEC-09: Internal error details leaked to GraphQL clients
**Location:** `backend/crates/api/src/graphql/query.rs`, `mutation.rs` — throughout

All errors are forwarded via `.map_err(|e| Error::new(e.to_string()))`, exposing database errors, file paths, and internal service details.

**Fix:** Map errors to generic user-facing messages; log details server-side.

---

### SEC-10: No rate limiting on any endpoint
**Location:** `backend/crates/api/src/main.rs`

No rate limiting middleware exists. The `force_sync` mutation triggers external HTTP calls, enabling abuse of external API rate limits and resource exhaustion.

**Fix:** Add `tower::limit::RateLimitLayer` or similar middleware.

---

### SEC-11: SSRF via user-controlled `jira.base_url` configuration
**Location:** `backend/crates/api/src/graphql/mutation.rs:202-211`

Users can set `jira.base_url` to any URL (including internal network addresses like `http://169.254.169.254/`) via the `update_configuration` mutation. The sync engine then makes HTTP requests to that URL.

**Fix:** Validate `jira.base_url` against an allowlist of external domains; reject private/internal IPs.

---

### BUG-07: Mutation errors silently swallowed across hooks
**Location:** Multiple frontend hooks (`use-triage.ts`, `use-priority-matrix.ts`, `use-task-edit.ts`, `DashboardPage.tsx`)

Mutations never check results for errors and unconditionally refetch, leaving users unaware of failures.

**Fix:** Check mutation `error` field and display toast notifications on failure.

---

## 3. MEDIUM Findings

### DESIGN-01: No transactions around multi-step database operations
**Location:** `backend/crates/infrastructure/src/database/task_repo.rs`

`save` + `save_task_tags` runs as separate statements without a transaction. A crash between DELETE tags and INSERT tags loses all tag associations. Same issue affects `save_batch`, `upsert_batch` (meetings), and sync flows.

### DESIGN-02: No input validation on domain types
**Location:** `backend/crates/domain/src/types/task.rs`, `meeting.rs`, `activity.rs`

All structs have public fields with no constructors. Negative hours, inverted date ranges, empty titles, and invalid confidence scores are all accepted.

### DESIGN-03: Silent fallback on invalid enum values in conversion functions
**Location:** `backend/crates/infrastructure/src/database/conversions.rs`

Every `*_from_str` function has a `_ =>` arm returning a default. Corrupted data is silently accepted.

### DESIGN-04: O(n^2) deduplication algorithm with no limit
**Location:** `backend/crates/application/src/use_cases/deduplication.rs:58`

Compares every pair of active tasks. 1000 tasks = ~500,000 comparisons with no pagination.

### DESIGN-05: `find_jira_key_in_text` matches substrings, causing false dedup positives
**Location:** `backend/crates/domain/src/rules/dedup.rs:13-15`

`text.contains(jira_key)` matches substrings like `"FOO-1234X"` containing `"FOO-123"`.

### DESIGN-06: Duplicated `parse_datetime` function (7 identical copies)
**Location:** `backend/crates/infrastructure/src/database/` — all repo files

### DESIGN-07: N+1 query pattern for loading task tags
**Location:** `backend/crates/infrastructure/src/database/task_repo.rs:141-149`

Tags loaded one-per-task in a loop instead of a batch `WHERE task_id IN (...)` query.

### SEC-12: No GraphQL query depth or complexity limits
**Location:** `backend/crates/api/src/graphql/schema.rs:44-59`

No `.limit_depth()` or `.limit_complexity()` — enables DoS via crafted queries.

### SEC-13: Unbounded `first` parameter — fetch-all-then-paginate
**Location:** `backend/crates/api/src/graphql/query.rs`

`tasks` and `alerts` queries fetch ALL records into memory then slice. A client can request `first: 2147483647`.

### SEC-14: Secrets exposed via `configuration` query
**Location:** `backend/crates/api/src/graphql/query.rs:339-353`

The `configuration` query returns all key-value pairs including `jira.token` and `outlook.access_token`.

### DESIGN-08: Axum version mismatch (code 0.8 vs specs/CLAUDE.md 0.7)
**Location:** `backend/crates/api/Cargo.toml:10`

### BUG-08: Outlook mapper ignores timezone field
**Location:** `backend/crates/infrastructure/src/connectors/outlook/mapper.rs:8-19`

All timestamps treated as UTC regardless of the calendar's actual timezone.

### BUG-09: JQL injection via unescaped project keys
**Location:** `backend/crates/infrastructure/src/connectors/jira/client.rs:45-74`

Project keys and assignee names with double quotes can manipulate JQL queries.

### BUG-10: SharePoint path not URL-encoded in Excel client
**Location:** `backend/crates/infrastructure/src/connectors/excel/client.rs:36`

Special characters in path or sheet name are interpolated directly into the URL.

### BUG-11: No request timeout on Outlook and Excel HTTP clients
**Location:** `backend/crates/infrastructure/src/connectors/outlook/client.rs:19`, `excel/client.rs:18`

Only Jira client sets a 30-second timeout. Others can hang indefinitely.

### BUG-12: `meeting_hours()` returns negative for inverted time ranges
**Location:** `backend/crates/domain/src/rules/workload.rs:6-8`

If `end_time < start_time`, the function returns a negative duration.

### FE-01: `as never` cast suppresses type checking on SSE subscriptions
**Location:** `frontend/src/lib/urql-client.ts:20`

### FE-02: Unsafe `as QuadrantKey` / `as string` casts on drag event IDs
**Location:** `frontend/src/components/priority/PriorityGrid.tsx:107`, `DashboardPage.tsx:409,431`

### FE-03: `parseFloat` without `isNaN` guard in task forms
**Location:** `frontend/src/components/task/TaskCreateSheet.tsx:63`, `TaskEditSheet.tsx:86,90,96`

### FE-04: No input length limits on any text field
**Location:** Multiple components — title, description, project name, etc.

### FE-05: Settings numeric fields accept negative and unreasonable values
**Location:** `frontend/src/pages/SettingsPage.tsx:728-750`

### FE-06: API response trusted without runtime validation
**Location:** `frontend/src/infrastructure/api-client.ts:46`

### FE-07: Missing `updated_at` column on `meetings` table
**Location:** `migrations/sqlite/001_initial.sql`

### FE-08: Missing index on `activity_slots.task_id`
**Location:** `migrations/sqlite/001_initial.sql:74-83`

### FE-09: Two parallel app structures coexist (dead code)
**Location:** `frontend/src/main.tsx` + `App.tsx` vs `frontend/src/presentation/main.tsx` + `app.tsx`

---

## 4. LOW Findings

| # | Issue | Location |
|---|-------|----------|
| L1 | 4 `DomainError` variants are dead code (never returned) | `domain/src/errors.rs:9-16` |
| L2 | `SimilarityScore`, `AlertData`, `ScheduledItem` lack `Debug`/`Clone` derives | `domain/src/rules/dedup.rs:2`, `alerts.rs:9,18` |
| L3 | `Quadrant` sort order depends on variant declaration order (fragile) | `domain/src/types/common.rs:83-89` |
| L4 | `fetch_all` used instead of `fetch_optional` for single-row lookups | `infrastructure/src/database/` (multiple repos) |
| L5 | Server binds to `0.0.0.0` instead of `127.0.0.1` | `api/src/main.rs:69` |
| L6 | `async-trait` is dev-dependency only in API crate | `api/Cargo.toml:24-25` |
| L7 | Spec uses `manuel` for source; code uses `personal` | `SPEC_FONCTIONNELLE.md:774` |
| L8 | Spec crate short names (`app`, `infra`) differ from code (`application`, `infrastructure`) | `SPEC_TECHNIQUE.md:89,98` |
| L9 | Index-based keys in list rendering | `frontend/src/pages/WorkloadPage.tsx:253,280` |
| L10 | Missing memoization on sort/filter in multiple pages | `DashboardPage.tsx`, `TriagePage.tsx`, `presentation/timeline.tsx` |
| L11 | `useEffect` on object reference restarts timer | `frontend/src/components/activity/ActivityTimer.tsx:49` |
| L12 | Suppressed ESLint exhaustive-deps with fragile workaround | `frontend/src/pages/DashboardPage.tsx:349-350` |
| L13 | `toLocaleTimeString` without explicit locale | `MeetingCard.tsx:12`, `DashboardPage.tsx:68` |
| L14 | Non-null assertions on DOM elements | `frontend/src/main.tsx:8`, `presentation/main.tsx:6` |
| L15 | Missing index on `task_links.task_id_secondary` | `migrations/sqlite/001_initial.sql:47-56` |
| L16 | No configurable server host/port env variable | `backend/.env.example` |
| L17 | Microsoft Graph token requires manual rotation | `backend/.env.example:10` |
| L18 | `jira_remaining_seconds` is `i32` but represents a duration (should be `u32`) | `domain/src/types/task.rs:27-29` |
| L19 | `estimated_hours` is `f32` while workload calculations use `f64` | Various files |

---

## 5. Recommended Fix Priority

### Immediate (before any deployment)
1. **SEC-01/02:** Wire authentication and restrict CORS
2. **SEC-03:** Add `'outlook'` to SQL CHECK constraints
3. **SEC-08:** Add authorization checks to all entity operations
4. **SEC-09:** Stop leaking internal errors to clients
5. **SEC-11/14:** Validate config keys (allowlist), redact secrets from queries

### Short-term (next sprint)
6. **DESIGN-01:** Wrap multi-step operations in transactions
7. **BUG-01:** Fix overload alert daily-vs-weekly comparison
8. **BUG-02:** Filter completed tasks from deadline alerts
9. **BUG-03:** Fix sync status stuck at "Syncing"
10. **SEC-05/06:** Add error boundary and fix API client error handling
11. **BUG-04/05/06:** Fix frontend state management bugs

### Medium-term
12. **DESIGN-02:** Add validation constructors to domain types
13. **DESIGN-03:** Return errors instead of silent fallbacks in conversions
14. **SEC-10/12/13:** Add rate limiting, query depth limits, pagination caps
15. **DESIGN-05/06/07:** Fix dedup matching, extract shared utilities, fix N+1 queries
