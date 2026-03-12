# Spec Review — Aggregated Plan v2

**Date:** 2026-03-07
**Scope:** Internal consistency review of SPEC_FONCTIONNELLE.md and SPEC_TECHNIQUE.md
**Context:** Both specs describe a complete rewrite replacing the existing TypeScript/Hono prototype with Rust/Axum + React/GraphQL.

---

## Review Outcome

The two specs are **largely consistent** — all 26 business rules, all MVP user stories, and all entity fields are properly mapped between functional and technical specifications. The sync/dedup/alert engine designs are coherent.

However, **12 changes** are required before implementation can begin.

---

## Required Changes

### 1. Task Scheduling — Hour-Based Slots (Critical)

**Problem:** The functional spec assumes tasks can be planned on specific time slots, but the Task domain type only has `deadline: Option<NaiveDate>`. Workload calculation, conflict detection, and the HalfDayGrid component all depend on task scheduling data that doesn't exist.

**Decision:** Tasks use **hour-based time slots**, not half-days. Half-day granularity is reserved for project assignment (developer-to-project allocation).

**Changes required:**
- Add to Task type: `planned_start: Option<DateTime>`, `planned_end: Option<DateTime>`, `estimated_hours: Option<f32>`
- Tasks are visually rendered at a size proportional to their `estimated_hours`
- Update R01: half-day applies to project assignment; tasks use hour slots
- Update R18/US-052: conflict detection uses time-range overlap, not half-day collision
- Update `check_conflict_alerts` signature: use `(Task, DateTime, DateTime)` instead of `(Task, HalfDay)`
- Update WorkloadPage: HalfDayGrid shows tasks sized by estimate within time slots

**Specs affected:** Both

### 2. Remove Dual Dedup Source of Truth

**Problem:** Task deduplication is tracked in two places: `linked_task_id` on Task AND the `task_links` table. This creates ambiguity.

**Decision:** Remove `linked_task_id` from the Task struct and database schema. Use the `task_links` table exclusively.

**Changes required:**
- Remove `linked_task_id` from Task struct (domain)
- Remove `linked_task_id` column from `tasks` table (migration)
- Remove `linkedTask` field from Task GraphQL type
- All dedup queries go through `task_links`

**Specs affected:** Technique

### 3. Multi-User Readiness — Document the Decision

**Problem:** The functional spec explicitly excludes multi-user (section 3.3, 4.2), but the tech spec adds `user_id` on all tables and includes auth middleware. This is intentional but undocumented.

**Decision:** Keep multi-user readiness. Document it.

**Changes required:**
- Add to functional spec section 11.3 (Decisions): "D6: Architecture multi-user ready (`user_id` on all tables, auth middleware) to prepare future Microsoft Teams deployment. Single-user in MVP — default user auto-created."

**Specs affected:** Fonctionnelle

### 4. Tags — Move to MVP

**Problem:** Functional spec labels tags as "Could (v2)" (US-064), but the tech spec includes full tag support in MVP (domain type, DB schema, GraphQL CRUD, referenced in CreateTaskInput).

**Decision:** Tags are MVP scope.

**Changes required:**
- Change US-064 priority from "Could (v2)" to "Must (MVP v1)"
- Move tags section from 6.7 (v2) to 6.5 or a new MVP section

**Specs affected:** Fonctionnelle

### 5. Project-Meeting Association — Auto-detect + Manual Override

**Problem:** US-061 says meetings are associated to projects "if the project name is in the meeting title", but the logic isn't specified in the tech spec, and there's no manual override mechanism.

**Decision:** Auto-detect project from meeting title during Outlook sync. Allow user to manually change or set the association.

**Changes required:**
- Add to Outlook sync mapper: after mapping, scan meeting title for known project names, set `project_id` if found
- Add GraphQL mutation: `updateMeetingProject(meetingId: ID!, projectId: ID): Meeting!`
- Add UI affordance on MeetingCard to change project association

**Specs affected:** Both

### 6. Jira Status — Display Raw Status

**Problem:** The tech spec maps only 3 Jira statuses (To Do, In Progress, Done), ignoring custom statuses and the app's "Blocked" status.

**Decision:** Display the raw Jira status string. Don't map to the app's TaskStatus enum for Jira-sourced tasks.

**Changes required:**
- Add to Task type: `jira_status: Option<String>`
- For Jira-sourced tasks, populate `jira_status` with the raw status name from Jira
- The app's `status` field (TaskStatus enum) is used for personal tasks and as a normalized view
- Frontend TaskCard shows `jira_status` badge for Jira tasks
- Add to Jira response mapping table in tech spec section 9.1

**Specs affected:** Technique

### 7. Week Starts Monday

**Problem:** `week_start_of(date)` function is called but the week start day is never defined.

**Decision:** Weeks start on Monday.

**Changes required:**
- Define in both specs: "A week runs Monday to Friday (5 business days, 10 half-days)"
- Implement `week_start_of` using Monday as start
- Add to glossary

**Specs affected:** Both

### 8. Meeting Time Model — No Half-Day Assignment

**Problem:** `check_conflict_alerts` takes `meetings_by_half_day: &[(Meeting, HalfDay)]`, implying each meeting maps to exactly one half-day. Meetings can span half-days (e.g., 11:00-14:00).

**Decision:** Meetings use their actual DateTime start/end for conflict detection. Half-day is only for project assignment.

**Changes required:**
- Rewrite `check_conflict_alerts` to use time-range overlap detection: two items conflict if their `[start, end)` intervals overlap
- Remove `meetings_by_half_day` parameter pattern
- Update R03: meeting consumption calculation uses actual duration against total available hours, not half-day assignment

**Specs affected:** Technique

### 9. Cursor-Based Pagination

**Problem:** No GraphQL query supports pagination. Open-ended queries (all tasks, alerts history) can grow unbounded.

**Decision:** Cursor-based pagination on unbounded queries. Day-bounded queries stay simple.

**Changes required:**
- Add to GraphQL schema:
  ```graphql
  type PageInfo {
    hasNextPage: Boolean!
    endCursor: String
  }
  type TaskConnection {
    edges: [TaskEdge!]!
    pageInfo: PageInfo!
    totalCount: Int!
  }
  type TaskEdge {
    node: Task!
    cursor: String!
  }
  ```
- Update queries: `tasks(filter, first: Int = 50, after: String): TaskConnection!`
- Same pattern for `alerts` query
- `dailyDashboard`, `activityJournal(date)`, `weeklyWorkload(weekStart)` remain unpaginated (bounded by date)

**Specs affected:** Technique

### 10. Configurable Working Hours

**Problem:** Section 13.3 hardcodes "no reminders outside 08:00-17:00" but working hours aren't configurable.

**Decision:** Make working hours configurable.

**Changes required:**
- Add to configuration parameters: `working_hours_start` (default: `"08:00"`), `working_hours_end` (default: `"17:00"`)
- Add to functional spec section 8.2 (configuration data)
- Add to tech spec section 15.1 (configuration parameters)
- Reminder suppression logic uses these values instead of hardcoded hours

**Specs affected:** Both

### 11. MeetingRepository — Add find_by_project

**Problem:** US-061 (project consolidated view) needs meetings filtered by project, but MeetingRepository only has date-based queries.

**Decision:** Add the missing repository method.

**Changes required:**
- Add to `MeetingRepository` trait:
  ```rust
  async fn find_by_project(
      &self, user_id: UserId, project_id: ProjectId,
  ) -> Result<Vec<Meeting>, RepositoryError>;
  ```
- Implement in SQLite repository
- Add index: `CREATE INDEX idx_meetings_project ON meetings(project_id);`

**Specs affected:** Technique

### 12. CLAUDE.md — Update for New Stack

**Problem:** CLAUDE.md describes the existing TypeScript/Hono codebase which is being replaced.

**Decision:** CLAUDE.md must be rewritten to reflect the Rust/Axum + React/GraphQL stack after the rewrite begins.

**Specs affected:** CLAUDE.md (during implementation)

---

## No Issues Found (Consistent)

- All 26 business rules (R01-R26) accounted for in tech spec
- All MVP user stories have corresponding GraphQL operations
- Entity field coverage between specs is complete
- Sync engine design (scheduler, coordinator, idempotency, local override preservation)
- Dedup engine design (auto-merge by Jira key, similarity scoring, user confirmation)
- Alert engine design (lifecycle, severity levels, auto-resolve)
- Database schema matches domain types
- Phased MVP implementation order is logical and independently testable
- Security model (local mode + future Teams) is coherent
- Frontend architecture (urql, codegen, component hierarchy) aligns with backend API
