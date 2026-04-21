# Task Edit & Jira Time Tracking Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable task editing via a slide-in sheet panel, display Jira time tracking data on task cards, and allow local override of planning fields.

**Architecture:** Three layers of changes: (1) Backend adds 5 new Task fields (3 Jira time + 2 overrides) with computed `effective_*` methods, wired through Jira connector → sync → GraphQL. (2) Frontend introduces a unified `TaskCard` component with `compact` prop and a `TaskEditSheet` using a right-side panel. (3) All pages (Dashboard, Triage, Priority Matrix) integrate the unified components.

**Tech Stack:** Rust (domain/application/infrastructure/api crates), SQLite (sqlx), async-graphql, React 18, TypeScript, urql, Tailwind CSS, @dnd-kit/core.

---

## File Structure

### Backend (files to modify)

| File | Responsibility |
|------|---------------|
| `backend/crates/domain/src/types/task.rs` | Add 5 new fields to `Task` struct + `effective_*` computed methods |
| `backend/crates/application/src/services/jira_client.rs` | Add 3 time fields to `JiraTask` DTO |
| `backend/crates/infrastructure/src/connectors/jira/types.rs` | Add 3 time fields to `JiraIssueFields` deserialization |
| `backend/crates/infrastructure/src/connectors/jira/client.rs` | Add time fields to API request field list |
| `backend/crates/infrastructure/src/connectors/jira/mapper.rs` | Extract time fields in `map_jira_issue` |
| `backend/crates/application/src/use_cases/sync.rs` | Map Jira time fields during sync (new + existing tasks) |
| `backend/crates/application/src/use_cases/task_management.rs` | Handle override fields in `UpdateTaskInput` + init in `create_personal_task` |
| `backend/crates/infrastructure/src/database/task_repo.rs` | Read/write 5 new columns in `map_task_row` and `save` |
| `backend/crates/api/src/graphql/types/task.rs` | Add new fields + computed resolvers + `UpdateTaskInput` fields |
| `backend/crates/api/src/graphql/mutation.rs` | Handle override fields in `convert_update_input` |

### Backend (files to create)

| File | Responsibility |
|------|---------------|
| `migrations/sqlite/003_add_time_tracking.sql` | ALTER TABLE for 5 new columns (repo root, after existing 001 + 002) |

### Frontend (files to modify)

| File | Responsibility |
|------|---------------|
| `frontend/src/components/task/TaskCard.tsx` | Rewrite as unified component with `compact` prop + time tracking display |
| `frontend/src/components/priority/QuadrantColumn.tsx` | Use unified TaskCard (compact) + click-to-edit callback |
| `frontend/src/hooks/use-priority-matrix.ts` | Add time + source fields to MatrixTask + query |
| `frontend/src/hooks/use-triage.ts` | Add time fields to TriageTask + query |
| `frontend/src/hooks/use-dashboard.ts` | Add time fields to DashboardTask + query |
| `frontend/src/pages/DashboardPage.tsx` | Integrate TaskEditSheet + pass onEdit to TaskCard |
| `frontend/src/pages/TriagePage.tsx` | Integrate TaskEditSheet + pass onEdit |
| `frontend/src/pages/PriorityMatrixPage.tsx` | Integrate TaskEditSheet |

### Frontend (files to create)

| File | Responsibility |
|------|---------------|
| `frontend/src/components/task/TaskEditSheet.tsx` | Right-side panel for editing task fields |
| `frontend/src/hooks/use-task-edit.ts` | Hook for fetching single task + update mutation |

---

## Chunk 1: Backend Data Model & Jira Connector

### Task 1: Database Migration

**Files:**
- Create: `migrations/sqlite/003_add_time_tracking.sql`

- [ ] **Step 1: Create the migration file**

```sql
-- 003_add_time_tracking.sql
-- Add Jira time tracking fields and local override fields to tasks table.
ALTER TABLE tasks ADD COLUMN jira_remaining_seconds INTEGER;
ALTER TABLE tasks ADD COLUMN jira_original_estimate_seconds INTEGER;
ALTER TABLE tasks ADD COLUMN jira_time_spent_seconds INTEGER;
ALTER TABLE tasks ADD COLUMN remaining_hours_override REAL;
ALTER TABLE tasks ADD COLUMN estimated_hours_override REAL;
```

- [ ] **Step 2: Verify the migration is picked up**

Run: `cd backend && cargo check -p infrastructure 2>&1 | head -30`
Expected: Compiles (sqlx uses runtime queries, not compile-time checked). The `sqlx::migrate!` macro in `infrastructure/src/database/connection.rs` will auto-discover this file.

- [ ] **Step 3: Commit**

```bash
git add migrations/sqlite/003_add_time_tracking.sql
git commit -m "feat: add time tracking migration (003)"
```

### Task 2: Domain Task Struct + Computed Methods

**Files:**
- Modify: `backend/crates/domain/src/types/task.rs`

- [ ] **Step 1: Write failing tests for effective_* methods**

Add to the bottom of `backend/crates/domain/src/types/task.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_test_task() -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            title: "Test".to_string(),
            description: None,
            source: Source::Jira,
            source_id: Some("PROJ-1".to_string()),
            jira_status: Some("In Progress".to_string()),
            status: TaskStatus::InProgress,
            project_id: None,
            assignee: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn effective_remaining_hours_override_takes_precedence() {
        let mut task = make_test_task();
        task.jira_remaining_seconds = Some(7200); // 2h
        task.remaining_hours_override = Some(5.0);
        assert_eq!(task.effective_remaining_hours(), Some(5.0));
    }

    #[test]
    fn effective_remaining_hours_falls_back_to_jira() {
        let mut task = make_test_task();
        task.jira_remaining_seconds = Some(3600); // 1h
        assert_eq!(task.effective_remaining_hours(), Some(1.0));
    }

    #[test]
    fn effective_remaining_hours_none_when_no_data() {
        let task = make_test_task();
        assert_eq!(task.effective_remaining_hours(), None);
    }

    #[test]
    fn effective_estimated_hours_override_takes_precedence() {
        let mut task = make_test_task();
        task.jira_original_estimate_seconds = Some(14400); // 4h
        task.estimated_hours_override = Some(8.0);
        assert_eq!(task.effective_estimated_hours(), Some(8.0));
    }

    #[test]
    fn effective_estimated_hours_falls_back_to_jira() {
        let mut task = make_test_task();
        task.jira_original_estimate_seconds = Some(14400); // 4h
        assert_eq!(task.effective_estimated_hours(), Some(4.0));
    }

    #[test]
    fn effective_estimated_hours_falls_back_to_estimated_hours() {
        let mut task = make_test_task();
        task.estimated_hours = Some(3.5);
        assert_eq!(task.effective_estimated_hours(), Some(3.5));
    }

    #[test]
    fn effective_estimated_hours_none_when_no_data() {
        let task = make_test_task();
        assert_eq!(task.effective_estimated_hours(), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test -p domain -- tests 2>&1 | tail -20`
Expected: FAIL — fields `jira_remaining_seconds` etc. don't exist yet on `Task`.

- [ ] **Step 3: Add 5 new fields to Task struct**

In `backend/crates/domain/src/types/task.rs`, add these fields to the `Task` struct after `tracking_state`:

```rust
    pub jira_remaining_seconds: Option<i32>,
    pub jira_original_estimate_seconds: Option<i32>,
    pub jira_time_spent_seconds: Option<i32>,
    pub remaining_hours_override: Option<f32>,
    pub estimated_hours_override: Option<f32>,
```

- [ ] **Step 4: Add impl Task with computed methods**

Add after the `Task` struct definition, before `#[cfg(test)]`:

```rust
impl Task {
    /// Effective remaining hours: local override > Jira remaining > None
    pub fn effective_remaining_hours(&self) -> Option<f32> {
        self.remaining_hours_override
            .or(self.jira_remaining_seconds.map(|s| s as f32 / 3600.0))
    }

    /// Effective estimated hours: local override > Jira estimate > estimated_hours (personal tasks)
    pub fn effective_estimated_hours(&self) -> Option<f32> {
        self.estimated_hours_override
            .or(self.jira_original_estimate_seconds.map(|s| s as f32 / 3600.0))
            .or(self.estimated_hours)
    }
}
```

- [ ] **Step 5: Fix all compilation errors across the workspace**

Every place that constructs a `Task` struct literal must now include the 5 new fields. There are **14 sites across 8 files**. Add `jira_remaining_seconds: None, jira_original_estimate_seconds: None, jira_time_spent_seconds: None, remaining_hours_override: None, estimated_hours_override: None` to each:

**Production code:**
1. `application/src/use_cases/task_management.rs` — `create_personal_task` function (~line 54)
2. `application/src/use_cases/sync.rs` — `sync_jira` new task creation (~line 119). (These `None`s will be replaced by Jira values in Task 4.)
3. `application/src/use_cases/sync.rs` — `sync_excel` new task creation (~line 376)
4. `infrastructure/src/database/task_repo.rs` — `map_task_row` function (~line 84). Temporarily set to `None` — Task 6 will wire them to the database columns.

**Test helpers and test code (set all 5 to `None`):**
5. `application/src/use_cases/dashboard.rs` — 4 `Task {` literals in tests (~lines 418, 442, 517, 581)
6. `application/src/use_cases/priority.rs` — `make_task` test helper (~line 222)
7. `application/src/use_cases/alerts.rs` — `make_task_with_deadline` test helper (~line 384)
8. `application/src/use_cases/deduplication.rs` — `make_task` test helper (~line 358)
9. `infrastructure/src/database/task_repo.rs` — `make_task` test helper (~line 440)
10. `infrastructure/src/database/task_repo.rs` — `save_and_read_tracking_state` test (~line 761)
11. `infrastructure/src/database/task_link_repo.rs` — `make_task` test helper (~line 156)

Note: `task_management.rs` tests use `create_personal_task` (not direct `Task {}` construction), so they compile without changes.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd backend && cargo test -p domain -- tests 2>&1 | tail -20`
Expected: All 7 new tests PASS.

Run: `cd backend && cargo check 2>&1 | tail -10`
Expected: Full workspace compiles.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/domain/src/types/task.rs backend/crates/application/src/use_cases/task_management.rs backend/crates/application/src/use_cases/sync.rs backend/crates/infrastructure/src/database/task_repo.rs
git add -u  # catch any other files that needed Task struct fix
git commit -m "feat(domain): add time tracking fields and effective_* methods to Task"
```

### Task 3: Jira Connector — Time Fields

**Files:**
- Modify: `backend/crates/application/src/services/jira_client.rs`
- Modify: `backend/crates/infrastructure/src/connectors/jira/types.rs`
- Modify: `backend/crates/infrastructure/src/connectors/jira/client.rs`
- Modify: `backend/crates/infrastructure/src/connectors/jira/mapper.rs`

- [ ] **Step 1: Write failing mapper test**

In `backend/crates/infrastructure/src/connectors/jira/mapper.rs`, update the `make_issue` test helper to include the 3 new fields **defaulting to `None`** (so existing tests are unaffected), then add a new test that sets them explicitly:

Update `make_issue` — add `timeestimate: None, timespent: None, timeoriginalestimate: None` to the `JiraIssueFields` struct literal (after `project`). Keep the existing signature unchanged.

Add test:

```rust
    #[test]
    fn maps_time_tracking_fields() {
        let mut issue = make_issue("PROJ-42", "Fix bug", "In Progress", None, None, None);
        issue.fields.timeestimate = Some(7200);
        issue.fields.timespent = Some(3600);
        issue.fields.timeoriginalestimate = Some(14400);
        let task = map_jira_issue(issue);

        assert_eq!(task.time_estimate_seconds, Some(7200));
        assert_eq!(task.time_spent_seconds, Some(3600));
        assert_eq!(task.time_original_estimate_seconds, Some(14400));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test -p infrastructure -- mapper::tests 2>&1 | tail -20`
Expected: FAIL — `timeestimate` field doesn't exist on `JiraIssueFields`.

- [ ] **Step 3: Add time fields to JiraIssueFields**

In `backend/crates/infrastructure/src/connectors/jira/types.rs`, add to `JiraIssueFields` struct after `project`:

```rust
    pub timeestimate: Option<i32>,
    pub timespent: Option<i32>,
    pub timeoriginalestimate: Option<i32>,
```

- [ ] **Step 4: Add time fields to JiraTask DTO**

In `backend/crates/application/src/services/jira_client.rs`, add to `JiraTask` struct after `project_name`:

```rust
    pub time_estimate_seconds: Option<i32>,
    pub time_spent_seconds: Option<i32>,
    pub time_original_estimate_seconds: Option<i32>,
```

- [ ] **Step 5: Update mapper to extract time fields**

In `backend/crates/infrastructure/src/connectors/jira/mapper.rs`, update `map_jira_issue`:

```rust
pub fn map_jira_issue(issue: JiraIssue) -> JiraTask {
    JiraTask {
        key: issue.key,
        title: issue.fields.summary,
        description: issue.fields.description,
        status: issue.fields.status.name,
        assignee: issue.fields.assignee.map(|a| a.display_name),
        deadline: issue.fields.duedate.and_then(|d| d.parse().ok()),
        priority: issue.fields.priority.map(|p| p.name),
        project_key: issue.fields.project.key,
        project_name: issue.fields.project.name,
        time_estimate_seconds: issue.fields.timeestimate,
        time_spent_seconds: issue.fields.timespent,
        time_original_estimate_seconds: issue.fields.timeoriginalestimate,
    }
}
```

- [ ] **Step 6: Add time fields to Jira API request**

In `backend/crates/infrastructure/src/connectors/jira/client.rs`, update the `fields` array in `fetch_page` (~line 82):

```rust
            fields: &["summary", "status", "assignee", "priority", "duedate", "project", "timeestimate", "timespent", "timeoriginalestimate"],
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure -- mapper::tests 2>&1 | tail -20`
Expected: All mapper tests PASS (including new `maps_time_tracking_fields`).

Run: `cd backend && cargo check 2>&1 | tail -10`
Expected: Full workspace compiles.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/application/src/services/jira_client.rs backend/crates/infrastructure/src/connectors/jira/types.rs backend/crates/infrastructure/src/connectors/jira/client.rs backend/crates/infrastructure/src/connectors/jira/mapper.rs
git commit -m "feat(jira): fetch time tracking fields from Jira API"
```

### Task 4: Sync — Map Jira Time Fields

**Files:**
- Modify: `backend/crates/application/src/use_cases/sync.rs`

- [ ] **Step 1: Update sync_jira to map time fields**

In `backend/crates/application/src/use_cases/sync.rs`, update the `sync_jira` function.

For **existing tasks** (the `Some(mut task) =>` branch, ~line 101), add after `task.project_id = project_id;` (~line 109):

```rust
                task.jira_remaining_seconds = jira_task.time_estimate_seconds;
                task.jira_original_estimate_seconds = jira_task.time_original_estimate_seconds;
                task.jira_time_spent_seconds = jira_task.time_spent_seconds;
                // Override fields are NOT touched by sync — user's local data preserved
```

For **new tasks** (the `None =>` branch, ~line 117), update the `Task { ... }` literal to set:

```rust
                    jira_remaining_seconds: jira_task.time_estimate_seconds,
                    jira_original_estimate_seconds: jira_task.time_original_estimate_seconds,
                    jira_time_spent_seconds: jira_task.time_spent_seconds,
                    remaining_hours_override: None,
                    estimated_hours_override: None,
```

(Replace the `None` placeholders added in Task 2 Step 5.)

- [ ] **Step 2: Verify compilation**

Run: `cd backend && cargo check 2>&1 | tail -10`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/application/src/use_cases/sync.rs
git commit -m "feat(sync): map Jira time tracking fields during sync"
```

### Task 5: Task Management — Override Fields

**Files:**
- Modify: `backend/crates/application/src/use_cases/task_management.rs`

- [ ] **Step 1: Write failing test for override update**

Add to the `tests` module in `backend/crates/application/src/use_cases/task_management.rs`:

```rust
    #[tokio::test]
    async fn update_task_with_time_overrides() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Jira Task".to_string(),
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let created = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();

        assert!(created.remaining_hours_override.is_none());
        assert!(created.estimated_hours_override.is_none());

        let update = UpdateTaskInput {
            title: None,
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: Some(Some(4.5)),
            estimated_hours_override: Some(Some(8.0)),
        };

        let updated = update_task(&repo, created.id, update, today())
            .await
            .unwrap();

        assert_eq!(updated.remaining_hours_override, Some(4.5));
        assert_eq!(updated.estimated_hours_override, Some(8.0));
    }

    #[tokio::test]
    async fn update_task_clear_time_overrides() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Task".to_string(),
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            impact: None,
            urgency: None,
            tags: vec![],
        };

        let created = create_personal_task(&repo, test_user_id(), input, today())
            .await
            .unwrap();

        // Set overrides
        let update1 = UpdateTaskInput {
            title: None,
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: Some(Some(4.5)),
            estimated_hours_override: Some(Some(8.0)),
        };
        let t = update_task(&repo, created.id, update1, today()).await.unwrap();
        assert_eq!(t.remaining_hours_override, Some(4.5));

        // Clear overrides with Some(None)
        let update2 = UpdateTaskInput {
            title: None,
            description: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: Some(None),
            estimated_hours_override: Some(None),
        };
        let cleared = update_task(&repo, created.id, update2, today()).await.unwrap();
        assert!(cleared.remaining_hours_override.is_none());
        assert!(cleared.estimated_hours_override.is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test -p application -- task_management::tests::update_task_with_time_overrides 2>&1 | tail -20`
Expected: FAIL — `remaining_hours_override` field doesn't exist on `UpdateTaskInput`.

- [ ] **Step 3: Add override fields to UpdateTaskInput**

In `backend/crates/application/src/use_cases/task_management.rs`, add to `UpdateTaskInput` struct after `tags`:

```rust
    pub remaining_hours_override: Option<Option<f32>>,
    pub estimated_hours_override: Option<Option<f32>>,
```

- [ ] **Step 4: Handle override fields in update_task function**

In the `update_task` function, add after the `tags` handling (~line 149):

```rust
    if let Some(remaining) = input.remaining_hours_override {
        task.remaining_hours_override = remaining;
    }
    if let Some(estimated) = input.estimated_hours_override {
        task.estimated_hours_override = estimated;
    }
```

- [ ] **Step 5: Fix all UpdateTaskInput construction sites**

Every place that constructs `UpdateTaskInput` must include the 2 new fields. Add `remaining_hours_override: None, estimated_hours_override: None` to:

1. `api/src/graphql/mutation.rs` — `convert_update_input` function (~line 610)
2. All test `UpdateTaskInput` literals in `task_management.rs` tests

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd backend && cargo test -p application -- task_management::tests 2>&1 | tail -30`
Expected: All tests PASS (including 2 new override tests).

Run: `cd backend && cargo check 2>&1 | tail -10`
Expected: Full workspace compiles.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/application/src/use_cases/task_management.rs backend/crates/api/src/graphql/mutation.rs
git commit -m "feat(application): add time override fields to UpdateTaskInput"
```

### Task 6: Database Repository — Read/Write New Columns

**Files:**
- Modify: `backend/crates/infrastructure/src/database/task_repo.rs`

- [ ] **Step 1: Write failing test for time field persistence**

Add to `tests` module in `backend/crates/infrastructure/src/database/task_repo.rs`:

```rust
    #[tokio::test]
    async fn save_and_read_time_tracking_fields() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut task = make_task("Time Tracked");
        task.source = Source::Jira;
        task.source_id = Some("PROJ-42".to_string());
        task.jira_remaining_seconds = Some(7200);
        task.jira_original_estimate_seconds = Some(14400);
        task.jira_time_spent_seconds = Some(3600);
        task.remaining_hours_override = Some(5.0);
        task.estimated_hours_override = Some(10.0);

        repo.save(&task).await.unwrap();

        let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(loaded.jira_remaining_seconds, Some(7200));
        assert_eq!(loaded.jira_original_estimate_seconds, Some(14400));
        assert_eq!(loaded.jira_time_spent_seconds, Some(3600));
        assert_eq!(loaded.remaining_hours_override, Some(5.0));
        assert_eq!(loaded.estimated_hours_override, Some(10.0));
    }

    #[tokio::test]
    async fn save_and_read_time_tracking_nulls() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let task = make_task("No Time Data");
        repo.save(&task).await.unwrap();

        let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert!(loaded.jira_remaining_seconds.is_none());
        assert!(loaded.jira_original_estimate_seconds.is_none());
        assert!(loaded.jira_time_spent_seconds.is_none());
        assert!(loaded.remaining_hours_override.is_none());
        assert!(loaded.estimated_hours_override.is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test -p infrastructure -- task_repo::tests::save_and_read_time_tracking 2>&1 | tail -20`
Expected: FAIL — fields are all `None` because `map_task_row` doesn't read them yet.

- [ ] **Step 3: Update map_task_row to read new columns**

In `backend/crates/infrastructure/src/database/task_repo.rs`, in the `map_task_row` function, add after the `tracking_state` reading (~line 82) and before `Ok(Task {`:

```rust
    let jira_remaining_seconds: Option<i32> = Row::try_get(row, "jira_remaining_seconds").ok().flatten();
    let jira_original_estimate_seconds: Option<i32> = Row::try_get(row, "jira_original_estimate_seconds").ok().flatten();
    let jira_time_spent_seconds: Option<i32> = Row::try_get(row, "jira_time_spent_seconds").ok().flatten();
    let remaining_hours_override: Option<f64> = Row::try_get(row, "remaining_hours_override").ok().flatten();
    let estimated_hours_override: Option<f64> = Row::try_get(row, "estimated_hours_override").ok().flatten();
```

Then in the `Ok(Task { ... })` block, replace the `None` placeholders with:

```rust
        jira_remaining_seconds,
        jira_original_estimate_seconds,
        jira_time_spent_seconds,
        remaining_hours_override: remaining_hours_override.map(|v| v as f32),
        estimated_hours_override: estimated_hours_override.map(|v| v as f32),
```

Note: Uses `try_get` with `.ok().flatten()` for backward compatibility with databases that haven't run the migration yet (same pattern as `tracking_state`).

- [ ] **Step 4: Update save method to write new columns**

In the `save` method, update the SQL INSERT statement to include the 5 new columns. Replace the current INSERT statement:

```rust
        sqlx::query(
            "INSERT OR REPLACE INTO tasks (id, user_id, title, description, source, source_id, jira_status, status, project_id, assignee, deadline, planned_start, planned_end, estimated_hours, urgency, urgency_manual, impact, tracking_state, jira_remaining_seconds, jira_original_estimate_seconds, jira_time_spent_seconds, remaining_hours_override, estimated_hours_override, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
```

And add 5 new `.bind()` calls after `.bind(task.tracking_state.to_string())`:

```rust
        .bind(task.jira_remaining_seconds)
        .bind(task.jira_original_estimate_seconds)
        .bind(task.jira_time_spent_seconds)
        .bind(task.remaining_hours_override.map(|h| h as f64))
        .bind(task.estimated_hours_override.map(|h| h as f64))
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test -p infrastructure -- task_repo::tests 2>&1 | tail -30`
Expected: All tests PASS (including 2 new time tracking tests).

- [ ] **Step 6: Commit**

```bash
git add backend/crates/infrastructure/src/database/task_repo.rs
git commit -m "feat(infrastructure): persist time tracking fields in SQLite"
```

### Task 7: GraphQL API — New Fields + Resolvers

**Files:**
- Modify: `backend/crates/api/src/graphql/types/task.rs`
- Modify: `backend/crates/api/src/graphql/mutation.rs`

- [ ] **Step 1: Add resolvers to TaskGql**

In `backend/crates/api/src/graphql/types/task.rs`, add these resolver methods to `impl TaskGql` after the `tracking_state` resolver:

```rust
    async fn jira_remaining_seconds(&self) -> Option<i32> {
        self.0.jira_remaining_seconds
    }

    async fn jira_original_estimate_seconds(&self) -> Option<i32> {
        self.0.jira_original_estimate_seconds
    }

    async fn jira_time_spent_seconds(&self) -> Option<i32> {
        self.0.jira_time_spent_seconds
    }

    async fn remaining_hours_override(&self) -> Option<f64> {
        self.0.remaining_hours_override.map(|h| h as f64)
    }

    async fn estimated_hours_override(&self) -> Option<f64> {
        self.0.estimated_hours_override.map(|h| h as f64)
    }

    /// Computed: local override > Jira remaining > None
    async fn effective_remaining_hours(&self) -> Option<f64> {
        self.0.effective_remaining_hours().map(|h| h as f64)
    }

    /// Computed: local override > Jira estimate > estimated_hours
    async fn effective_estimated_hours(&self) -> Option<f64> {
        self.0.effective_estimated_hours().map(|h| h as f64)
    }
```

- [ ] **Step 2: Add override fields to GraphQL UpdateTaskInput**

In `backend/crates/api/src/graphql/types/task.rs`, add to the `UpdateTaskInput` struct after `tag_ids`.

These must support 3 states: "don't change" (field absent), "set value" (`{ remainingHoursOverride: 4.5 }`), and "clear override" (`{ remainingHoursOverride: null }`). async-graphql's `MaybeUndefined<T>` handles this natively, but to keep things simple and consistent with the existing `Option<Option<T>>` pattern used for `description`/`deadline`, we use `Option<Option<f64>>`:

```rust
    /// Set to Some(Some(val)) to override, Some(None) to clear, None to leave unchanged.
    pub remaining_hours_override: Option<Option<f64>>,
    /// Set to Some(Some(val)) to override, Some(None) to clear, None to leave unchanged.
    pub estimated_hours_override: Option<Option<f64>>,
```

Note: In async-graphql, `Option<Option<f64>>` maps to a nullable field where:
- Field omitted from input → outer `None` (don't change)
- Field set to `null` → `Some(None)` (clear override)
- Field set to a number → `Some(Some(val))` (set override)

- [ ] **Step 3: Wire override fields in convert_update_input**

In `backend/crates/api/src/graphql/mutation.rs`, in `convert_update_input`, add after the `tags` handling (before the closing `Ok(task_management::UpdateTaskInput {`):

```rust
        remaining_hours_override: match input.remaining_hours_override {
            Some(Some(h)) => Some(Some(h as f32)),
            Some(None) => Some(None),
            None => None,
        },
        estimated_hours_override: match input.estimated_hours_override {
            Some(Some(h)) => Some(Some(h as f32)),
            Some(None) => Some(None),
            None => None,
        },
```

This preserves the full 3-state semantics: `None` = don't change, `Some(None)` = clear override, `Some(Some(val))` = set override value.

- [ ] **Step 4: Verify compilation**

Run: `cd backend && cargo check 2>&1 | tail -10`
Expected: Compiles.

- [ ] **Step 5: Run all backend tests**

Run: `cd backend && cargo test 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/api/src/graphql/types/task.rs backend/crates/api/src/graphql/mutation.rs
git commit -m "feat(api): expose time tracking fields and overrides via GraphQL"
```

---

## Chunk 2: Frontend — TaskEditSheet, Unified TaskCard, Hook Updates

### Task 8: Task Edit Hook

**Files:**
- Create: `frontend/src/hooks/use-task-edit.ts`

- [ ] **Step 1: Create the hook file**

```typescript
import { useQuery, useMutation } from 'urql';

export interface FullTask {
  readonly id: string;
  readonly title: string;
  readonly description: string | null;
  readonly source: string;
  readonly sourceId: string | null;
  readonly status: string;
  readonly jiraStatus: string | null;
  readonly urgency: string;   // GraphQL enum: LOW, MEDIUM, HIGH, CRITICAL
  readonly impact: string;    // GraphQL enum: LOW, MEDIUM, HIGH, CRITICAL
  readonly quadrant: string;
  readonly deadline: string | null;
  readonly assignee: string | null;
  readonly estimatedHours: number | null;
  readonly trackingState: string;
  readonly jiraRemainingSeconds: number | null;
  readonly jiraOriginalEstimateSeconds: number | null;
  readonly jiraTimeSpentSeconds: number | null;
  readonly remainingHoursOverride: number | null;
  readonly estimatedHoursOverride: number | null;
  readonly effectiveRemainingHours: number | null;
  readonly effectiveEstimatedHours: number | null;
  readonly project: { readonly name: string } | null;
  readonly tags: readonly { readonly id: string; readonly name: string; readonly color: string | null }[];
}

const TASK_QUERY = `
  query GetTask($id: ID!) {
    task(id: $id) {
      id
      title
      description
      source
      sourceId
      status
      jiraStatus
      urgency
      impact
      quadrant
      deadline
      assignee
      estimatedHours
      trackingState
      jiraRemainingSeconds
      jiraOriginalEstimateSeconds
      jiraTimeSpentSeconds
      remainingHoursOverride
      estimatedHoursOverride
      effectiveRemainingHours
      effectiveEstimatedHours
      project { name }
      tags { id name color }
    }
  }
`;

const UPDATE_TASK_MUTATION = `
  mutation UpdateTask($id: ID!, $input: UpdateTaskInput!) {
    updateTask(id: $id, input: $input) {
      id
      title
      description
      urgency
      impact
      quadrant
      estimatedHours
      remainingHoursOverride
      estimatedHoursOverride
      effectiveRemainingHours
      effectiveEstimatedHours
      tags { id name color }
    }
  }
`;

const UPDATE_PRIORITY_MUTATION = `
  mutation UpdateTaskPriority($taskId: ID!, $urgency: UrgencyLevelGql, $impact: ImpactLevelGql) {
    updatePriority(taskId: $taskId, urgency: $urgency, impact: $impact) {
      id urgency impact quadrant
    }
  }
`;

export function useTaskEdit(taskId: string | null) {
  const [result, reexecute] = useQuery<{ task: FullTask }>({
    query: TASK_QUERY,
    variables: { id: taskId },
    pause: !taskId,
    requestPolicy: 'cache-and-network',
  });

  const [, executeUpdate] = useMutation(UPDATE_TASK_MUTATION);
  const [, executePriorityUpdate] = useMutation(UPDATE_PRIORITY_MUTATION);

  const updateTask = async (input: Record<string, unknown>) => {
    if (!taskId) return;
    await executeUpdate({ id: taskId, input });
    reexecute({ requestPolicy: 'network-only' });
  };

  const updatePriority = async (urgency: string, impact: string) => {
    if (!taskId) return;
    await executePriorityUpdate({ taskId, urgency, impact });
    reexecute({ requestPolicy: 'network-only' });
  };

  return {
    task: result.data?.task ?? null,
    loading: result.fetching,
    error: result.error ?? null,
    updateTask,
    updatePriority,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit 2>&1 | tail -20`
Expected: Compiles (or existing errors unrelated to this file).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/use-task-edit.ts
git commit -m "feat(frontend): add use-task-edit hook for task editing"
```

### Task 9: TaskEditSheet Component

**Files:**
- Create: `frontend/src/components/task/TaskEditSheet.tsx`

- [ ] **Step 1: Create the TaskEditSheet component**

Since shadcn/ui Sheet is not installed, we'll build a lightweight slide-in panel with Tailwind. This avoids adding a dependency for a single component.

```typescript
import { useState, useEffect, useCallback } from 'react';
import { useTaskEdit } from '@/hooks/use-task-edit';

interface TaskEditSheetProps {
  readonly taskId: string | null;
  readonly onClose: () => void;
  readonly onUpdated?: () => void;
}

const URGENCY_OPTIONS = [
  { value: 'LOW', label: 'Low' },
  { value: 'MEDIUM', label: 'Medium' },
  { value: 'HIGH', label: 'High' },
  { value: 'CRITICAL', label: 'Critical' },
] as const;

const IMPACT_OPTIONS = [
  { value: 'LOW', label: 'Low' },
  { value: 'MEDIUM', label: 'Medium' },
  { value: 'HIGH', label: 'High' },
  { value: 'CRITICAL', label: 'Critical' },
] as const;

/** GraphQL returns urgency/impact as enum strings (LOW, MEDIUM, HIGH, CRITICAL). */
function normalizeEnum(val: string): string {
  const upper = String(val).toUpperCase();
  if (['LOW', 'MEDIUM', 'HIGH', 'CRITICAL'].includes(upper)) return upper;
  return 'MEDIUM';
}

function formatSeconds(seconds: number | null): string {
  if (seconds === null || seconds === undefined) return '-';
  const hours = seconds / 3600;
  if (hours < 1) return `${Math.round(seconds / 60)}m`;
  return `${hours.toFixed(1)}h`;
}

export function TaskEditSheet({ taskId, onClose, onUpdated }: TaskEditSheetProps) {
  const { task, loading, updateTask, updatePriority } = useTaskEdit(taskId);
  const isOpen = taskId !== null;
  const isJira = task?.source === 'JIRA' || task?.source === 'EXCEL';

  // Local form state
  const [description, setDescription] = useState('');
  const [estimatedHours, setEstimatedHours] = useState('');
  const [remainingOverride, setRemainingOverride] = useState('');
  const [estimatedOverride, setEstimatedOverride] = useState('');
  const [urgency, setUrgency] = useState('MEDIUM');
  const [impact, setImpact] = useState('MEDIUM');

  // Sync form state when task loads
  useEffect(() => {
    if (task) {
      setDescription(task.description ?? '');
      setEstimatedHours(task.estimatedHours?.toString() ?? '');
      setRemainingOverride(task.remainingHoursOverride?.toString() ?? '');
      setEstimatedOverride(task.estimatedHoursOverride?.toString() ?? '');
      setUrgency(normalizeEnum(task.urgency));
      setImpact(normalizeEnum(task.impact));
    }
  }, [task]);

  const handleSave = useCallback(async () => {
    if (!task) return;

    // Update urgency/impact via priority mutation
    const currentUrgency = normalizeEnum(task.urgency);
    const currentImpact = normalizeEnum(task.impact);
    if (urgency !== currentUrgency || impact !== currentImpact) {
      await updatePriority(urgency, impact);
    }

    // Build update input for other fields
    const input: Record<string, unknown> = {};

    const newDesc = description || null;
    if (newDesc !== (task.description ?? null)) {
      input.description = newDesc;
    }

    if (isJira) {
      // Override fields for Jira/Excel tasks
      const newRemaining = remainingOverride ? parseFloat(remainingOverride) : null;
      if (newRemaining !== task.remainingHoursOverride) {
        input.remainingHoursOverride = newRemaining;
      }
      const newEstOverride = estimatedOverride ? parseFloat(estimatedOverride) : null;
      if (newEstOverride !== task.estimatedHoursOverride) {
        input.estimatedHoursOverride = newEstOverride;
      }
    } else {
      // Personal tasks: write directly to estimatedHours
      const newEst = estimatedHours ? parseFloat(estimatedHours) : null;
      if (newEst !== task.estimatedHours) {
        input.estimatedHours = newEst;
      }
    }

    if (Object.keys(input).length > 0) {
      await updateTask(input);
    }

    onUpdated?.();
    onClose();
  }, [task, description, estimatedHours, remainingOverride, estimatedOverride, urgency, impact, isJira, updateTask, updatePriority, onUpdated, onClose]);

  // Close on Escape
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown);
      return () => document.removeEventListener('keydown', handleKeyDown);
    }
  }, [isOpen, onClose]);

  return (
    <>
      {/* Backdrop */}
      {isOpen && (
        <div
          className="fixed inset-0 bg-black/20 z-40 transition-opacity"
          onClick={onClose}
        />
      )}

      {/* Sheet panel */}
      <div
        className={`fixed top-0 right-0 h-full w-full max-w-md bg-white shadow-xl z-50 transform transition-transform duration-200 ease-in-out ${
          isOpen ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        {isOpen && (
          <div className="flex flex-col h-full">
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-gray-200">
              <div className="flex items-center gap-2 min-w-0">
                {task?.sourceId && (
                  <span className="text-xs font-mono font-medium text-blue-600 flex-shrink-0">
                    {task.sourceId}
                  </span>
                )}
                <h2 className="text-base font-semibold text-gray-900 truncate">
                  {task?.title ?? 'Loading...'}
                </h2>
              </div>
              <button
                onClick={onClose}
                className="p-1.5 text-gray-400 hover:text-gray-600 rounded-md hover:bg-gray-100 transition-colors"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-5">
              {loading && !task ? (
                <div className="flex items-center justify-center py-12">
                  <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
                </div>
              ) : task ? (
                <>
                  {/* Read-only info section */}
                  <div className="space-y-2">
                    <div className="flex items-center gap-2 text-sm text-gray-600">
                      <span className="font-medium w-20">Status:</span>
                      <span className="px-2 py-0.5 bg-gray-100 rounded text-xs font-medium">
                        {task.status.replace('_', ' ')}
                      </span>
                      {task.jiraStatus && (
                        <span className="px-2 py-0.5 bg-blue-50 text-blue-700 rounded text-xs font-medium border border-blue-200">
                          {task.jiraStatus}
                        </span>
                      )}
                    </div>
                    {task.assignee && (
                      <div className="flex items-center gap-2 text-sm text-gray-600">
                        <span className="font-medium w-20">Assignee:</span>
                        <span>{task.assignee}</span>
                      </div>
                    )}
                    {task.deadline && (
                      <div className="flex items-center gap-2 text-sm text-gray-600">
                        <span className="font-medium w-20">Deadline:</span>
                        <span>{task.deadline}</span>
                      </div>
                    )}
                    {task.project?.name && (
                      <div className="flex items-center gap-2 text-sm text-gray-600">
                        <span className="font-medium w-20">Project:</span>
                        <span>{task.project.name}</span>
                      </div>
                    )}
                  </div>

                  {/* Jira time tracking (read-only display) */}
                  {isJira && (task.jiraOriginalEstimateSeconds !== null || task.jiraTimeSpentSeconds !== null || task.jiraRemainingSeconds !== null) && (
                    <div className="bg-blue-50 rounded-lg p-3 space-y-1.5">
                      <h4 className="text-xs font-semibold text-blue-800 uppercase tracking-wider">Jira Time Tracking</h4>
                      <div className="grid grid-cols-3 gap-2 text-center">
                        <div>
                          <p className="text-xs text-blue-600">Estimate</p>
                          <p className="text-sm font-medium text-blue-900">{formatSeconds(task.jiraOriginalEstimateSeconds)}</p>
                        </div>
                        <div>
                          <p className="text-xs text-blue-600">Logged</p>
                          <p className="text-sm font-medium text-blue-900">{formatSeconds(task.jiraTimeSpentSeconds)}</p>
                        </div>
                        <div>
                          <p className="text-xs text-blue-600">Remaining</p>
                          <p className="text-sm font-medium text-blue-900">{formatSeconds(task.jiraRemainingSeconds)}</p>
                        </div>
                      </div>
                    </div>
                  )}

                  {/* Editable fields */}
                  <div className="space-y-4">
                    <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Priority</h4>

                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <label className="block text-xs font-medium text-gray-700 mb-1">Urgency</label>
                        <select
                          value={urgency}
                          onChange={(e) => setUrgency(e.target.value)}
                          className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        >
                          {URGENCY_OPTIONS.map(o => (
                            <option key={o.value} value={o.value}>{o.label}</option>
                          ))}
                        </select>
                      </div>
                      <div>
                        <label className="block text-xs font-medium text-gray-700 mb-1">Impact</label>
                        <select
                          value={impact}
                          onChange={(e) => setImpact(e.target.value)}
                          className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        >
                          {IMPACT_OPTIONS.map(o => (
                            <option key={o.value} value={o.value}>{o.label}</option>
                          ))}
                        </select>
                      </div>
                    </div>

                    <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Time Estimates</h4>

                    {isJira ? (
                      <div className="grid grid-cols-2 gap-3">
                        <div>
                          <label className="block text-xs font-medium text-gray-700 mb-1">
                            Remaining (h) <span className="text-gray-400">override</span>
                          </label>
                          <input
                            type="number"
                            step="0.5"
                            min="0"
                            value={remainingOverride}
                            onChange={(e) => setRemainingOverride(e.target.value)}
                            placeholder={task.jiraRemainingSeconds !== null ? formatSeconds(task.jiraRemainingSeconds) : '-'}
                            className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                          />
                        </div>
                        <div>
                          <label className="block text-xs font-medium text-gray-700 mb-1">
                            Estimate (h) <span className="text-gray-400">override</span>
                          </label>
                          <input
                            type="number"
                            step="0.5"
                            min="0"
                            value={estimatedOverride}
                            onChange={(e) => setEstimatedOverride(e.target.value)}
                            placeholder={task.jiraOriginalEstimateSeconds !== null ? formatSeconds(task.jiraOriginalEstimateSeconds) : '-'}
                            className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                          />
                        </div>
                      </div>
                    ) : (
                      <div>
                        <label className="block text-xs font-medium text-gray-700 mb-1">Estimated hours</label>
                        <input
                          type="number"
                          step="0.5"
                          min="0"
                          value={estimatedHours}
                          onChange={(e) => setEstimatedHours(e.target.value)}
                          placeholder="e.g. 4"
                          className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                        />
                      </div>
                    )}

                    <div>
                      <label className="block text-xs font-medium text-gray-700 mb-1">Description</label>
                      <textarea
                        value={description}
                        onChange={(e) => setDescription(e.target.value)}
                        rows={4}
                        className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 resize-none"
                        placeholder="Add a description..."
                      />
                    </div>
                  </div>
                </>
              ) : null}
            </div>

            {/* Footer */}
            <div className="px-5 py-3 border-t border-gray-200 flex items-center justify-end gap-2">
              <button
                onClick={onClose}
                className="px-3 py-1.5 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors"
              >
                Save
              </button>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
```

**Known gap:** Tags multi-select editing is not included in this TaskEditSheet. The design spec mentions tags as editable, but implementing a multi-select tag picker requires fetching available tags and is a separate concern. Tags display is included (read-only from task data); editing can be added in a follow-up iteration.

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/task/TaskEditSheet.tsx
git commit -m "feat(frontend): add TaskEditSheet slide-in panel component"
```

### Task 10: Unified TaskCard with Compact Prop

**Files:**
- Modify: `frontend/src/components/task/TaskCard.tsx`

- [ ] **Step 1: Rewrite TaskCard with compact prop and time tracking**

Replace the entire contents of `frontend/src/components/task/TaskCard.tsx`:

```typescript
import { SOURCE_COLORS, QUADRANT_LABELS } from '@/lib/constants';

interface TaskTag {
  readonly id: string;
  readonly name: string;
  readonly color?: string | null;
}

export interface TaskCardProps {
  readonly id: string;
  readonly title: string;
  readonly source: string;
  readonly sourceId?: string | null;
  readonly status: string;
  readonly jiraStatus?: string | null;
  readonly urgency: number;
  readonly impact: number;
  readonly quadrant: string;
  readonly deadline?: string | null;
  readonly assignee?: string | null;
  readonly projectName?: string | null;
  readonly tags?: readonly TaskTag[];
  readonly effectiveRemainingHours?: number | null;
  readonly effectiveEstimatedHours?: number | null;
  readonly jiraTimeSpentSeconds?: number | null;
  readonly compact?: boolean;
  readonly onClick?: () => void;
}

const STATUS_STYLES: Record<string, string> = {
  TODO: 'bg-gray-100 text-gray-700',
  IN_PROGRESS: 'bg-blue-100 text-blue-700',
  DONE: 'bg-green-100 text-green-700',
  BLOCKED: 'bg-red-100 text-red-700',
  CANCELLED: 'bg-gray-200 text-gray-500',
};

const QUADRANT_STYLES: Record<string, string> = {
  UrgentImportant: 'bg-red-100 text-red-800',
  Important: 'bg-yellow-100 text-yellow-800',
  Urgent: 'bg-orange-100 text-orange-800',
  Neither: 'bg-gray-100 text-gray-600',
};

function getSourceColor(source: string): string {
  return (SOURCE_COLORS as Record<string, string>)[source] ?? SOURCE_COLORS.PERSONAL;
}

function getQuadrantLabel(quadrant: string): string {
  return (QUADRANT_LABELS as Record<string, string>)[quadrant] ?? quadrant;
}

function formatHours(hours: number | null | undefined): string {
  if (hours === null || hours === undefined) return '-';
  if (hours < 1) return `${Math.round(hours * 60)}m`;
  return `${hours.toFixed(1)}h`;
}

function TimeTrackingRow({
  remaining,
  logged,
  estimate,
}: {
  readonly remaining: number | null | undefined;
  readonly logged: number | null | undefined;
  readonly estimate: number | null | undefined;
}) {
  if (remaining == null && logged == null && estimate == null) return null;

  const loggedHours = logged !== null && logged !== undefined ? logged / 3600 : null;
  const progressPct = estimate && loggedHours !== null ? Math.min((loggedHours / estimate) * 100, 100) : null;

  return (
    <div className="flex items-center gap-2 text-xs text-gray-500 mt-1">
      <svg className="w-3.5 h-3.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      <span title="Remaining">{formatHours(remaining)}</span>
      <span className="text-gray-300">/</span>
      <span title="Logged">{formatHours(loggedHours)}</span>
      <span className="text-gray-300">/</span>
      <span title="Estimate">{formatHours(estimate)}</span>
      {progressPct !== null && (
        <div className="flex-1 h-1.5 bg-gray-200 rounded-full overflow-hidden max-w-16">
          <div
            className={`h-full rounded-full ${progressPct >= 90 ? 'bg-red-400' : progressPct >= 70 ? 'bg-yellow-400' : 'bg-blue-400'}`}
            style={{ width: `${progressPct}%` }}
          />
        </div>
      )}
    </div>
  );
}

export function TaskCard({
  title,
  source,
  sourceId,
  status,
  jiraStatus,
  quadrant,
  deadline,
  assignee,
  projectName,
  tags,
  effectiveRemainingHours,
  effectiveEstimatedHours,
  jiraTimeSpentSeconds,
  compact = false,
  onClick,
}: TaskCardProps) {
  const sourceColor = getSourceColor(source);
  const statusStyle = STATUS_STYLES[status] ?? 'bg-gray-100 text-gray-700';

  if (compact) {
    return (
      <div
        className={`bg-white rounded-md border border-gray-200 p-2.5 hover:shadow-sm transition-shadow ${onClick ? 'cursor-pointer' : ''}`}
        onClick={onClick}
      >
        {/* Top row: source ID + remaining hours */}
        <div className="flex items-center justify-between gap-1 mb-1">
          <div className="flex items-center gap-1.5">
            <span
              className="inline-block w-2 h-2 rounded-full flex-shrink-0"
              style={{ backgroundColor: sourceColor }}
            />
            {sourceId && (
              <span className="text-xs font-mono font-medium text-blue-600">{sourceId}</span>
            )}
          </div>
          {effectiveRemainingHours !== null && effectiveRemainingHours !== undefined && (
            <span className="text-xs text-gray-500">{formatHours(effectiveRemainingHours)}</span>
          )}
        </div>
        {/* Title */}
        <h4 className="text-sm font-medium text-gray-900 mb-1 leading-tight truncate">{title}</h4>
        {/* Bottom row: status + assignee */}
        <div className="flex flex-wrap items-center gap-1.5">
          <span className={`inline-flex px-1.5 py-0.5 rounded text-xs font-medium ${statusStyle}`}>
            {status.replace('_', ' ')}
          </span>
          {assignee && (
            <span className="text-xs text-gray-400 truncate">{assignee}</span>
          )}
        </div>
      </div>
    );
  }

  // Full card
  const quadrantStyle = QUADRANT_STYLES[quadrant] ?? 'bg-gray-100 text-gray-600';
  const quadrantLabel = getQuadrantLabel(quadrant);

  return (
    <div
      className={`bg-white rounded-lg border border-gray-200 p-4 hover:shadow-sm transition-shadow ${onClick ? 'cursor-pointer' : ''}`}
      onClick={onClick}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-2">
            <span
              className="inline-block w-2 h-2 rounded-full flex-shrink-0"
              style={{ backgroundColor: sourceColor }}
              title={source}
            />
            {sourceId && (
              <span className="text-xs font-mono font-medium text-blue-600 flex-shrink-0">
                {sourceId}
              </span>
            )}
            <h3 className="text-sm font-medium text-gray-900 truncate">{title}</h3>
          </div>

          <div className="flex flex-wrap items-center gap-1.5 mb-2">
            <span className={`inline-flex px-2 py-0.5 rounded text-xs font-medium ${statusStyle}`}>
              {status.replace('_', ' ')}
            </span>
            {jiraStatus && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-medium bg-blue-50 text-blue-700 border border-blue-200">
                <svg className="w-3 h-3 flex-shrink-0" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M11.53 2c0 2.4 1.97 4.35 4.35 4.35h1.78v1.7c0 2.4 1.94 4.34 4.34 4.35V2.84a.84.84 0 00-.84-.84H11.53zM6.77 6.8a4.36 4.36 0 004.34 4.34h1.78v1.72a4.36 4.36 0 004.34 4.34V7.63a.84.84 0 00-.83-.83H6.77zM2 11.6c0 2.4 1.95 4.34 4.35 4.34h1.78v1.72c0 2.4 1.94 4.34 4.35 4.34v-9.57a.84.84 0 00-.84-.83H2z" />
                </svg>
                {jiraStatus}
              </span>
            )}
            <span className={`inline-flex px-2 py-0.5 rounded text-xs font-medium ${quadrantStyle}`}>
              {quadrantLabel}
            </span>
          </div>

          {/* Time tracking row */}
          <TimeTrackingRow
            remaining={effectiveRemainingHours}
            logged={jiraTimeSpentSeconds}
            estimate={effectiveEstimatedHours}
          />

          <div className="flex flex-wrap items-center gap-3 text-xs text-gray-500 mt-1">
            {deadline && (
              <span className="flex items-center gap-1">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5" />
                </svg>
                {deadline}
              </span>
            )}
            {assignee && (
              <span className="flex items-center gap-1">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z" />
                </svg>
                {assignee}
              </span>
            )}
            {projectName && (
              <span className="flex items-center gap-1">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z" />
                </svg>
                {projectName}
              </span>
            )}
          </div>

          {tags && tags.length > 0 && (
            <div className="flex flex-wrap gap-1 mt-2">
              {tags.map(tag => (
                <span
                  key={tag.id}
                  className="inline-flex px-1.5 py-0.5 rounded text-xs font-medium"
                  style={{
                    backgroundColor: tag.color ? `${tag.color}20` : '#E5E7EB',
                    color: tag.color ?? '#4B5563',
                  }}
                >
                  {tag.name}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/task/TaskCard.tsx
git commit -m "feat(frontend): unified TaskCard with compact prop and time tracking"
```

### Task 11: Update Frontend Hooks — Add Time Fields to Queries

**Files:**
- Modify: `frontend/src/hooks/use-dashboard.ts`
- Modify: `frontend/src/hooks/use-triage.ts`
- Modify: `frontend/src/hooks/use-priority-matrix.ts`

- [ ] **Step 1: Update DashboardTask and DASHBOARD_QUERY**

In `frontend/src/hooks/use-dashboard.ts`:

Add to `DashboardTask` interface after `tags`:
```typescript
  readonly effectiveRemainingHours: number | null;
  readonly effectiveEstimatedHours: number | null;
  readonly jiraTimeSpentSeconds: number | null;
```

Add to the DASHBOARD_QUERY `tasks { ... }` block after `tags { id name color }`:
```graphql
        effectiveRemainingHours
        effectiveEstimatedHours
        jiraTimeSpentSeconds
```

- [ ] **Step 2: Update TriageTask and TRIAGE_TASKS_QUERY**

In `frontend/src/hooks/use-triage.ts`:

Add to `TriageTask` interface after `project`:
```typescript
  readonly effectiveRemainingHours: number | null;
  readonly effectiveEstimatedHours: number | null;
  readonly jiraTimeSpentSeconds: number | null;
```

Add to the TRIAGE_TASKS_QUERY node fields after `project { name }`:
```graphql
          effectiveRemainingHours
          effectiveEstimatedHours
          jiraTimeSpentSeconds
```

- [ ] **Step 3: Update MatrixTask and PRIORITY_MATRIX_QUERY**

In `frontend/src/hooks/use-priority-matrix.ts`:

Add to `MatrixTask` interface after `project`:
```typescript
  readonly source: string;
  readonly sourceId: string | null;
  readonly jiraStatus: string | null;
  readonly effectiveRemainingHours: number | null;
  readonly effectiveEstimatedHours: number | null;
  readonly jiraTimeSpentSeconds: number | null;
```

Update the PRIORITY_MATRIX_QUERY — each quadrant selection needs the new fields. Replace every occurrence of:
```graphql
        id title status urgency impact deadline assignee
        project { name }
```
with:
```graphql
        id title status urgency impact deadline assignee source sourceId jiraStatus
        effectiveRemainingHours effectiveEstimatedHours jiraTimeSpentSeconds
        project { name }
```

- [ ] **Step 4: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/hooks/use-dashboard.ts frontend/src/hooks/use-triage.ts frontend/src/hooks/use-priority-matrix.ts
git commit -m "feat(frontend): add time tracking fields to all GraphQL queries"
```

---

## Chunk 3: Frontend — Page Integration

### Task 12: Dashboard Page — TaskEditSheet + Click-to-Edit

**Files:**
- Modify: `frontend/src/pages/DashboardPage.tsx`

- [ ] **Step 1: Integrate TaskEditSheet and onClick**

In `frontend/src/pages/DashboardPage.tsx`:

Add imports at the top:
```typescript
import { useState, useCallback } from 'react';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
```

(Replace the existing `useState` import from React.)

Add state + handler inside `DashboardPage` component, before the date state:
```typescript
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);

  const handleTaskClick = useCallback((taskId: string) => {
    setEditingTaskId(taskId);
  }, []);

  const handleSheetClose = useCallback(() => {
    setEditingTaskId(null);
  }, []);
```

Update the `taskCards` mapping to include time fields and onClick. Replace the `.map(t => ({` section to add:
```typescript
          effectiveRemainingHours: t.effectiveRemainingHours ?? null,
          effectiveEstimatedHours: t.effectiveEstimatedHours ?? null,
          jiraTimeSpentSeconds: t.jiraTimeSpentSeconds ?? null,
```

The `TaskList` component renders `TaskCard` internally. Since `TaskList` doesn't yet support `onClick`, we need to either:
(a) Pass `onEdit` through `TaskList` to individual `TaskCard` components, or
(b) Wrap `TaskCard` in the `DashboardPage` directly.

The simplest approach: add an `onEdit` prop to `TaskList` that gets passed through. Check `frontend/src/components/task/TaskList.tsx` and add an `onEdit` prop that maps `(taskId) => void` through to each `TaskCard`'s `onClick`.

Add `<TaskEditSheet>` at the end of the return JSX, just before the closing `</div>`. Pass `onUpdated` to trigger a dashboard refetch after edits:
```tsx
      <TaskEditSheet
        taskId={editingTaskId}
        onClose={handleSheetClose}
        onUpdated={() => {
          // urql cache-and-network will refetch on next render, but force it for immediate update
        }}
      />
```

Note: Since the dashboard uses `useQuery` (default `cache-first`), the sheet's mutation responses should include enough fields for urql's document cache to reflect changes. If stale data persists, switch the dashboard query to `cache-and-network` request policy (same pattern as priority matrix).

- [ ] **Step 2: Update TaskList to support onEdit**

In `frontend/src/components/task/TaskList.tsx`, add `onEdit` prop:

```typescript
interface TaskListProps {
  readonly tasks: readonly TaskCardProps[];
  readonly emptyMessage?: string;
  readonly onEdit?: (taskId: string) => void;
}
```

Pass `onClick={() => onEdit?.(task.id)}` to each `TaskCard`.

- [ ] **Step 3: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/DashboardPage.tsx frontend/src/components/task/TaskList.tsx
git commit -m "feat(frontend): integrate TaskEditSheet into DashboardPage"
```

### Task 13: QuadrantColumn — Use Unified TaskCard + Click-to-Edit

**Files:**
- Modify: `frontend/src/components/priority/QuadrantColumn.tsx`

- [ ] **Step 1: Rewrite QuadrantColumn to use unified TaskCard**

Replace the component to use the shared `TaskCard` in compact mode. The `QuadrantTask` interface needs to include the new fields from `MatrixTask`.

Update the `QuadrantTask` interface:
```typescript
interface QuadrantTask {
  readonly id: string;
  readonly title: string;
  readonly status: string;
  readonly urgency: number;
  readonly impact: number;
  readonly deadline: string | null;
  readonly assignee: string | null;
  readonly project: { readonly name: string } | null;
  readonly source: string;
  readonly sourceId: string | null;
  readonly jiraStatus: string | null;
  readonly effectiveRemainingHours: number | null;
  readonly effectiveEstimatedHours: number | null;
  readonly jiraTimeSpentSeconds: number | null;
}
```

Add `onEdit` to `QuadrantColumnProps`:
```typescript
  readonly onEdit?: (taskId: string) => void;
```

In `DraggableTask`, import and use `TaskCard` in compact mode:
```typescript
import { TaskCard } from '@/components/task/TaskCard';

function DraggableTask({ task, onEdit }: { readonly task: QuadrantTask; readonly onEdit?: (taskId: string) => void }) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id: task.id,
  });

  return (
    <div
      ref={setNodeRef}
      {...listeners}
      {...attributes}
      className={`cursor-grab active:cursor-grabbing ${isDragging ? 'opacity-30' : ''}`}
    >
      <TaskCard
        id={task.id}
        title={task.title}
        source={task.source}
        sourceId={task.sourceId}
        status={task.status}
        jiraStatus={task.jiraStatus}
        urgency={task.urgency}
        impact={task.impact}
        quadrant=""
        deadline={task.deadline}
        assignee={task.assignee}
        projectName={task.project?.name ?? null}
        effectiveRemainingHours={task.effectiveRemainingHours}
        effectiveEstimatedHours={task.effectiveEstimatedHours}
        jiraTimeSpentSeconds={task.jiraTimeSpentSeconds}
        compact
        onClick={onEdit ? () => onEdit(task.id) : undefined}
      />
    </div>
  );
}
```

Rewrite `TaskCardOverlay` to use `TaskCard` compact:
```typescript
export function TaskCardOverlay({ task }: { readonly task: QuadrantTask }) {
  return (
    <div className="shadow-lg ring-2 ring-blue-300 rounded-md w-64">
      <TaskCard
        id={task.id}
        title={task.title}
        source={task.source}
        sourceId={task.sourceId}
        status={task.status}
        jiraStatus={task.jiraStatus}
        urgency={task.urgency}
        impact={task.impact}
        quadrant=""
        deadline={task.deadline}
        assignee={task.assignee}
        projectName={task.project?.name ?? null}
        effectiveRemainingHours={task.effectiveRemainingHours}
        effectiveEstimatedHours={task.effectiveEstimatedHours}
        jiraTimeSpentSeconds={task.jiraTimeSpentSeconds}
        compact
      />
    </div>
  );
}
```

Remove the old inline `TaskCardContent` function (no longer needed — `TaskCard` replaces it).

Pass `onEdit` through `QuadrantColumn` to `DraggableTask`.

- [ ] **Step 2: Update PriorityGrid to pass onEdit through**

In `frontend/src/components/priority/PriorityGrid.tsx`, add `onEdit` to `PriorityGridProps`:
```typescript
  readonly onEdit?: (taskId: string) => void;
```

Pass it to each `QuadrantColumn`.

- [ ] **Step 3: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/priority/QuadrantColumn.tsx frontend/src/components/priority/PriorityGrid.tsx
git commit -m "feat(frontend): QuadrantColumn uses unified TaskCard (compact)"
```

### Task 14: PriorityMatrixPage — TaskEditSheet Integration

**Files:**
- Modify: `frontend/src/pages/PriorityMatrixPage.tsx`

- [ ] **Step 1: Add TaskEditSheet and onEdit**

In `frontend/src/pages/PriorityMatrixPage.tsx`:

Add imports:
```typescript
import { useState, useCallback } from 'react';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
```

Add state inside `PriorityMatrixPage`:
```typescript
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);

  const handleEdit = useCallback((taskId: string) => {
    setEditingTaskId(taskId);
  }, []);
```

Pass `onEdit={handleEdit}` to `PriorityGrid`.

Also pass `onDragStart` to close the sheet when dragging starts. `PriorityGrid` already calls `handleDragStart` — add an `onDragStartExternal` prop to `PriorityGrid` and call it from `handleDragStart`:

In `PriorityGrid.tsx`, add to `PriorityGridProps`:
```typescript
  readonly onDragStartExternal?: () => void;
```

In `handleDragStart`, add at the top:
```typescript
    onDragStartExternal?.();
```

In `PriorityMatrixPage`, pass:
```typescript
  <PriorityGrid data={data} onMoveTask={handleMoveTask} onEdit={handleEdit} onDragStartExternal={() => setEditingTaskId(null)} />
```

Add `<TaskEditSheet>` to JSX:
```tsx
      <TaskEditSheet taskId={editingTaskId} onClose={() => setEditingTaskId(null)} />
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/PriorityMatrixPage.tsx
git commit -m "feat(frontend): integrate TaskEditSheet into PriorityMatrixPage"
```

### Task 15: TriagePage — TaskEditSheet Integration

**Files:**
- Modify: `frontend/src/pages/TriagePage.tsx`

- [ ] **Step 1: Rewrite DraggableTaskCard to use unified TaskCard**

In `frontend/src/pages/TriagePage.tsx`:

Add import:
```typescript
import { TaskCard } from '@/components/task/TaskCard';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
```

Update PointerSensor distance from `5` to `8` (consistent with PriorityGrid):
```typescript
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } })
  );
```

Add state inside `TriagePage`:
```typescript
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);
```

Rewrite `DraggableTaskCard` to use unified `TaskCard`:
```typescript
function DraggableTaskCard({
  task,
  onDismiss,
  onEdit,
}: {
  readonly task: TriageTask;
  readonly onDismiss?: () => void;
  readonly onEdit?: (taskId: string) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, isDragging } =
    useDraggable({ id: task.id });

  const style = {
    transform: transform ? `translate3d(${transform.x}px, ${transform.y}px, 0)` : undefined,
    opacity: isDragging ? 0.4 : 1,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      {...listeners}
      {...attributes}
      className="cursor-grab active:cursor-grabbing"
    >
      <TaskCard
        id={task.id}
        title={task.title}
        source={task.source}
        sourceId={task.sourceId}
        status={task.status}
        jiraStatus={task.jiraStatus ?? null}
        urgency={task.urgency}
        impact={task.impact}
        quadrant=""
        deadline={task.deadline}
        assignee={task.assignee}
        projectName={task.project?.name ?? null}
        effectiveRemainingHours={task.effectiveRemainingHours ?? null}
        effectiveEstimatedHours={task.effectiveEstimatedHours ?? null}
        jiraTimeSpentSeconds={task.jiraTimeSpentSeconds ?? null}
        onClick={onEdit ? () => onEdit(task.id) : undefined}
      />
      {onDismiss && (
        <button
          onClick={(e) => { e.stopPropagation(); onDismiss(); }}
          className="absolute top-2 right-2 p-1 text-gray-400 hover:text-red-500 transition-colors"
          title="Dismiss task"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </div>
  );
}
```

Note: The dismiss button is overlaid on top of the card. Wrap the `DraggableTaskCard` outer div with `className="relative"` so the absolute positioning works.

Rewrite `TaskCardOverlay` to use unified `TaskCard`:
```typescript
function TaskCardOverlay({ task }: { readonly task: TriageTask }) {
  return (
    <div className="shadow-lg ring-2 ring-blue-400 rounded-lg w-80">
      <TaskCard
        id={task.id}
        title={task.title}
        source={task.source}
        sourceId={task.sourceId}
        status={task.status}
        jiraStatus={task.jiraStatus ?? null}
        urgency={task.urgency}
        impact={task.impact}
        quadrant=""
        deadline={task.deadline}
        assignee={task.assignee}
        projectName={task.project?.name ?? null}
        effectiveRemainingHours={task.effectiveRemainingHours ?? null}
        effectiveEstimatedHours={task.effectiveEstimatedHours ?? null}
        jiraTimeSpentSeconds={task.jiraTimeSpentSeconds ?? null}
        compact
      />
    </div>
  );
}
```

Remove the inline `SourceDot` function (no longer needed).

- [ ] **Step 2: Wire onEdit and close-on-drag-start**

Pass `onEdit` to `DraggableTaskCard`:
```tsx
  <DraggableTaskCard
    key={task.id}
    task={task}
    onDismiss={() => dismissTask(task.id)}
    onEdit={(id) => setEditingTaskId(id)}
  />
```

Close the sheet when dragging starts — update `handleDragStart`:
```typescript
  const handleDragStart = (event: DragStartEvent) => {
    setEditingTaskId(null); // Close sheet on drag start
    const task = allTasks.find(t => t.id === event.active.id);
    setActiveTask(task ?? null);
  };
```

Add `<TaskEditSheet>` after `</DndContext>`:
```tsx
      <TaskEditSheet taskId={editingTaskId} onClose={() => setEditingTaskId(null)} />
```

- [ ] **Step 3: Verify TypeScript compilation**

Run: `cd frontend && npx tsc --noEmit 2>&1 | tail -20`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/TriagePage.tsx
git commit -m "feat(frontend): integrate TaskEditSheet into TriagePage with unified TaskCard"
```

### Task 16: Full Stack Verification

- [ ] **Step 1: Run all backend tests**

Run: `cd backend && cargo test 2>&1 | tail -30`
Expected: All tests pass.

- [ ] **Step 2: Run frontend type check**

Run: `cd frontend && npx tsc --noEmit 2>&1 | tail -20`
Expected: No errors.

- [ ] **Step 3: Build frontend**

Run: `cd frontend && pnpm build 2>&1 | tail -20`
Expected: Build succeeds.

- [ ] **Step 4: Start backend and verify GraphQL schema**

Run: `cd backend && cargo run -p api &` then:
```bash
curl -s http://localhost:3001/graphql -H 'Content-Type: application/json' -d '{"query":"{ __type(name: \"Task\") { fields { name type { name kind ofType { name } } } } }"}' | python3 -m json.tool | grep -E "jira|remaining|estimated|effective"
```

Expected: New fields appear in the schema: `jiraRemainingSeconds`, `jiraOriginalEstimateSeconds`, `jiraTimeSpentSeconds`, `remainingHoursOverride`, `estimatedHoursOverride`, `effectiveRemainingHours`, `effectiveEstimatedHours`.

- [ ] **Step 5: Update SPEC_FONCTIONNELLE.md**

Add to the task data model section (parcours 2 or wherever the task attributes are listed):
- `tempsRestantJira` (Jira remaining time in seconds)
- `tempsOriginalEstiméJira` (Jira original estimate in seconds)
- `tempsDépenséJira` (Jira time spent in seconds)
- `surchargeHeuresRestantes` (local override for remaining hours)
- `surchargeHeuresEstimées` (local override for estimated hours)

Add new user story:
- US-043: Édition de tâche via panneau latéral (from any screen)
- US-044: Affichage du suivi temporel Jira avec surcharge locale

- [ ] **Step 6: Update SPEC_TECHNIQUE.md**

Add time tracking fields to the Task domain type documentation. Add the `effective_*` computed methods. Document the `TaskEditSheet` component. Update the GraphQL schema section with the 7 new fields.

- [ ] **Step 7: Commit spec updates**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "docs: update specs with time tracking and task edit sheet"
```
