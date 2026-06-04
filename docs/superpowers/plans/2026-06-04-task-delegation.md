# Task Delegation Field Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a user-owned `delegated_to` free-text field to tasks, with auto-learned name suggestions, editable in the task sheet and shown on task cards.

**Architecture:** New nullable `delegated_to` column on `tasks`, threaded through domain → application (`UpdateTaskInput` with set/clear semantics, new `list_delegates` repo method) → GraphQL (`delegatedTo` field, `delegates: [String!]!` query) → React (datalist combobox in TaskEditSheet, badge on TaskCard). Sync never touches the field — same contract as `notes`. Purely informational: no workload/alert/matrix impact.

**Tech Stack:** Rust (Axum, async-graphql 7, sqlx 0.8 runtime queries, SQLite), React 18 + TypeScript strict + urql, Vitest/RTL.

**Spec:** `docs/superpowers/specs/2026-06-04-task-delegation-design.md`

**Conventions that matter here:**
- Run backend commands from `backend/` (`cd backend && cargo test -p <crate>`).
- Run frontend commands from `frontend/` (`cd frontend && pnpm test`).
- The workspace compiles as a whole: adding a struct field breaks every literal construction site at once. Task 1 fixes them all in one commit; later tasks stay compiling.
- `Option<Option<T>>` in update inputs means: `None` = leave unchanged, `Some(None)` = clear, `Some(Some(v))` = set. async-graphql maps GraphQL `null` → `Some(None)` and absent → `None` for `Option<Option<T>>` input fields (existing pattern: `remaining_hours_override`).

---

### Task 1: Migration + domain field + workspace plumbing

The field must exist everywhere before any behavior can be tested, because Rust struct literals are exhaustive. This task is mechanical plumbing; behavior tests come in Tasks 2–5.

**Files:**
- Create: `migrations/sqlite/008_add_delegated_to.sql`
- Modify: `backend/crates/domain/src/types/task.rs` (struct + test fixture)
- Modify: every `Task { ... }` literal the compiler reports (list below)

- [ ] **Step 1: Create the migration**

```sql
-- 008_add_delegated_to.sql
-- Person a task is delegated to (free text). User-owned: never overwritten by sync.
ALTER TABLE tasks ADD COLUMN delegated_to TEXT;
```

Save as `migrations/sqlite/008_add_delegated_to.sql`.

- [ ] **Step 2: Add the field to the domain Task struct**

In `backend/crates/domain/src/types/task.rs`, after the `assignee` field (line 21):

```rust
    pub assignee: Option<String>,
    /// Person this task is delegated to (free text). User-owned — never
    /// overwritten by Jira/Excel sync, unlike `assignee` which mirrors Jira.
    pub delegated_to: Option<String>,
```

- [ ] **Step 3: Find every broken construction site**

Run: `cd backend && cargo check 2>&1 | grep -E "^error" | head -40`
Expected: many `missing field 'delegated_to'` errors.

- [ ] **Step 4: Add `delegated_to: None` to every Task literal**

Add `delegated_to: None,` right after `assignee: ...,` in each literal. Known sites (the compiler is the authority — fix every site it reports, these are the ones identified up front):

- `backend/crates/domain/src/types/task.rs` — `make_test_task()` test helper
- `backend/crates/application/src/use_cases/task_management.rs` — `create_personal_task` (the one production literal: a brand-new personal task starts undelegated), plus test helpers `make_recurring_task` and the `cancelled_task` literal
- `backend/crates/application/src/use_cases/sync.rs` — Jira new-task literal (~line 138) and Excel new-task literal (~line 409)
- `backend/crates/application/src/use_cases/recurrence.rs` — materialized-instance literal (if present)
- `backend/crates/infrastructure/src/database/task_repo.rs` — test helpers `make_task()` and the full literal in `save_and_read_tracking_state`
- `backend/crates/api/src/graphql/tests.rs` — any Task literals in test helpers

**Exception** — in `backend/crates/infrastructure/src/database/task_repo.rs`, inside `map_task_row` (the row→Task mapper), use the real column read instead of `None`:

```rust
        assignee: Row::get(row, "assignee"),
        delegated_to: Row::try_get(row, "delegated_to").ok().flatten(),
```

(`try_get().ok().flatten()` is the established pattern for columns added by later migrations — see `notes` one line above.)

- [ ] **Step 5: Verify the workspace compiles and all tests pass**

Run: `cd backend && cargo check && cargo test`
Expected: clean check, all tests PASS (no behavior changed yet; `save()` does not persist the field yet — that is Task 2's red test).

- [ ] **Step 6: Commit**

```bash
git add migrations/sqlite/008_add_delegated_to.sql backend/
git commit -m "feat(domain): add delegated_to field to Task with migration"
```

---

### Task 2: SQLite persistence + `list_delegates` (TDD)

**Files:**
- Modify: `backend/crates/application/src/repositories/task_repository.rs` (trait)
- Modify: `backend/crates/infrastructure/src/database/task_repo.rs` (save SQL + new method + tests)

- [ ] **Step 1: Write the failing roundtrip test**

In `backend/crates/infrastructure/src/database/task_repo.rs`, in the existing `mod tests`, after `save_and_read_notes`:

```rust
    #[tokio::test]
    async fn save_and_read_delegated_to() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut task = make_task("Delegated");
        task.delegated_to = Some("Marie".to_string());
        repo.save(&task).await.unwrap();

        let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(loaded.delegated_to.as_deref(), Some("Marie"));

        // Clearing: save with None overwrites the previous value
        task.delegated_to = None;
        repo.save(&task).await.unwrap();
        let cleared = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert!(cleared.delegated_to.is_none());
    }
```

- [ ] **Step 2: Run it — must fail**

Run: `cd backend && cargo test -p infrastructure save_and_read_delegated_to`
Expected: FAIL — `loaded.delegated_to` is `None` because `save()` doesn't write the column.

- [ ] **Step 3: Persist the column in `save()`**

In the same file, in `save()`: add `delegated_to` to the column list right after `assignee`, add one `?` placeholder (28 → 29), and add the bind right after `.bind(&task.assignee)`:

```rust
            "INSERT OR REPLACE INTO tasks (id, user_id, title, description, notes, source, source_id, jira_status, status, project_id, assignee, delegated_to, deadline, planned_start, planned_end, estimated_hours, urgency, urgency_manual, impact, tracking_state, jira_remaining_seconds, jira_original_estimate_seconds, jira_time_spent_seconds, remaining_hours_override, estimated_hours_override, recurrence_id, occurrence_date, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
```

```rust
        .bind(&task.assignee)
        .bind(&task.delegated_to)
```

- [ ] **Step 4: Run it — must pass**

Run: `cd backend && cargo test -p infrastructure save_and_read_delegated_to`
Expected: PASS.

- [ ] **Step 5: Write the failing `list_delegates` test**

Same `mod tests`:

```rust
    #[tokio::test]
    async fn list_delegates_returns_distinct_sorted_names() {
        let pool = setup().await;
        let repo = SqliteTaskRepository::new(pool);

        let mut t1 = make_task("A");
        t1.delegated_to = Some("Marie".to_string());
        let mut t2 = make_task("B");
        t2.delegated_to = Some("Ahmed".to_string());
        let mut t3 = make_task("C");
        t3.delegated_to = Some("Marie".to_string()); // duplicate
        let t4 = make_task("D"); // not delegated
        for t in [&t1, &t2, &t3, &t4] {
            repo.save(t).await.unwrap();
        }

        let names = repo.list_delegates(user_id()).await.unwrap();
        assert_eq!(names, vec!["Ahmed".to_string(), "Marie".to_string()]);
    }
```

- [ ] **Step 6: Run it — must fail to compile**

Run: `cd backend && cargo test -p infrastructure list_delegates`
Expected: compile error — `list_delegates` not found.

- [ ] **Step 7: Add the trait method with a default impl**

In `backend/crates/application/src/repositories/task_repository.rs`, inside `trait TaskRepository`, after `delete_stale_by_source`:

```rust
    /// Distinct, sorted delegate names previously used on the user's tasks.
    /// Backs the auto-learned suggestion list for the delegation field.
    /// Default implementation returns an empty list; concrete repositories override.
    async fn list_delegates(&self, user_id: UserId) -> Result<Vec<String>, RepositoryError> {
        let _ = user_id;
        Ok(vec![])
    }
```

(Default impl keeps the existing in-memory test repos in `task_management.rs` compiling — same approach as `find_by_recurrence_slot`.)

- [ ] **Step 8: Implement it on `SqliteTaskRepository`**

In `backend/crates/infrastructure/src/database/task_repo.rs`, inside `impl TaskRepository for SqliteTaskRepository`:

```rust
    async fn list_delegates(&self, user_id: UserId) -> Result<Vec<String>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT DISTINCT delegated_to FROM tasks \
             WHERE user_id = ? AND delegated_to IS NOT NULL \
             ORDER BY delegated_to",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|row| Row::get(row, "delegated_to"))
            .collect())
    }
```

- [ ] **Step 9: Run tests — must pass**

Run: `cd backend && cargo test -p infrastructure && cargo test -p application`
Expected: all PASS.

- [ ] **Step 10: Commit**

```bash
git add backend/crates/application/src/repositories/task_repository.rs backend/crates/infrastructure/src/database/task_repo.rs
git commit -m "feat(infra): persist delegated_to and add list_delegates repository method"
```

---

### Task 3: Application `update_task` set/clear (TDD)

**Files:**
- Modify: `backend/crates/application/src/use_cases/task_management.rs` (input struct, update fn, tests)
- Modify: `backend/crates/api/src/graphql/mutation.rs` (`convert_update_input` — keep compiling with `None`; real mapping lands in Task 5)

- [ ] **Step 1: Write the failing set/clear test**

In `task_management.rs` `mod tests`, after `update_task_clear_time_overrides`. NOTE: `UpdateTaskInput` has no `Default` impl, so every literal is exhaustive — this test won't compile until Step 3 adds the field, and Step 3 will also break the ~12 existing `UpdateTaskInput` literals in this file (fix them by adding `delegated_to: None,`).

```rust
    #[tokio::test]
    async fn update_task_sets_and_clears_delegated_to() {
        let repo = InMemoryTaskRepository::new();
        let input = CreateTaskInput {
            title: "Task".to_string(),
            description: None,
            notes: None,
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
        assert!(created.delegated_to.is_none());

        // Set
        let set = UpdateTaskInput {
            title: None,
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            delegated_to: Some(Some("Marie".to_string())),
        };
        let updated = update_task(&repo, created.id, set, today()).await.unwrap();
        assert_eq!(updated.delegated_to.as_deref(), Some("Marie"));

        // Clear with Some(None)
        let clear = UpdateTaskInput {
            title: None,
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            delegated_to: Some(None),
        };
        let cleared = update_task(&repo, created.id, clear, today()).await.unwrap();
        assert!(cleared.delegated_to.is_none());
    }

    #[tokio::test]
    async fn update_task_recurring_instance_allows_delegated_to() {
        // Delegation is per-instance, not template-level: it must NOT be
        // rejected by the recurring-instance guard.
        let repo = InMemoryTaskRepository::new();
        let recurring = make_recurring_task(&repo, test_user_id(), None);

        let update = UpdateTaskInput {
            title: None,
            description: None,
            notes: None,
            project_id: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            status: None,
            impact: None,
            urgency: None,
            tags: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            delegated_to: Some(Some("Marie".to_string())),
        };
        let result = update_task(&repo, recurring.id, update, today()).await;
        assert!(result.is_ok(), "delegated_to on recurring instance should succeed");
        assert_eq!(result.unwrap().delegated_to.as_deref(), Some("Marie"));
    }
```

- [ ] **Step 2: Run — must fail to compile**

Run: `cd backend && cargo test -p application delegated_to`
Expected: compile error — no field `delegated_to` on `UpdateTaskInput`.

- [ ] **Step 3: Add the field and handling**

In `task_management.rs`, `UpdateTaskInput` struct — add after `estimated_hours_override`:

```rust
    pub estimated_hours_override: Option<Option<f32>>,
    /// Set to Some(Some(name)) to delegate, Some(None) to clear, None to leave unchanged.
    pub delegated_to: Option<Option<String>>,
```

In `update_task`, after the `estimated_hours_override` if-block (~line 196):

```rust
    if let Some(delegated_to) = input.delegated_to {
        task.delegated_to = delegated_to;
    }
```

Do **NOT** add `delegated_to` to `has_template_only_fields()` — it's per-instance (pinned by the second test above).

Fix the compile fallout:
- All existing `UpdateTaskInput { ... }` literals in `task_management.rs` tests: add `delegated_to: None,` to each.
- `backend/crates/api/src/graphql/mutation.rs`, `convert_update_input` (~line 997): add `delegated_to: None,` to the returned `task_management::UpdateTaskInput` literal for now (the GraphQL input field doesn't exist yet — Task 5 replaces this with the real mapping).

- [ ] **Step 4: Run — must pass**

Run: `cd backend && cargo test -p application && cargo check`
Expected: all PASS, workspace compiles.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/application/src/use_cases/task_management.rs backend/crates/api/src/graphql/mutation.rs
git commit -m "feat(application): set/clear delegated_to via update_task"
```

---

### Task 4: Sync-preservation pin test

Sync's update path mutates an explicit field list and never touches `delegated_to`, so this test should pass as written. It exists to pin the contract — if someone later rewrites the sync update to rebuild the whole Task, this fails loudly.

**Files:**
- Modify: `backend/crates/application/src/use_cases/sync.rs` (tests module only)

- [ ] **Step 1: Add minimal mocks + the pin test**

In `sync.rs` `mod tests` (currently only has the two status-mapping tests), append:

```rust
    use crate::errors::{ConnectorError, RepositoryError};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Returns one fixed Jira task ("AP-1") on every fetch.
    struct StubJiraClient;

    #[async_trait]
    impl JiraClient for StubJiraClient {
        async fn fetch_tasks(
            &self,
            _project_keys: &[String],
            _assignees: Option<&[String]>,
            _my_tasks_only: bool,
        ) -> Result<Vec<JiraTask>, ConnectorError> {
            Ok(vec![JiraTask {
                key: "AP-1".to_string(),
                title: "Synced title".to_string(),
                description: Some("Synced description".to_string()),
                status: "In Progress".to_string(),
                assignee: Some("jira.user@example.com".to_string()),
                deadline: None,
                priority: None,
                project_key: "AP".to_string(),
                project_name: "Aggregated Plan".to_string(),
                time_estimate_seconds: None,
                time_spent_seconds: None,
                time_original_estimate_seconds: None,
            }])
        }
    }

    /// Minimal in-memory TaskRepository covering only what sync_jira touches.
    #[derive(Default)]
    struct MiniTaskRepo {
        tasks: Mutex<HashMap<TaskId, Task>>,
    }

    #[async_trait]
    impl TaskRepository for MiniTaskRepo {
        async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
            Ok(self.tasks.lock().unwrap().get(&id).cloned())
        }
        async fn find_by_user(
            &self,
            user_id: UserId,
            _filter: &TaskFilter,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .values()
                .filter(|t| t.user_id == user_id)
                .cloned()
                .collect())
        }
        async fn find_by_source(
            &self,
            user_id: UserId,
            source: Source,
            source_id: &str,
        ) -> Result<Option<Task>, RepositoryError> {
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .values()
                .find(|t| {
                    t.user_id == user_id
                        && t.source == source
                        && t.source_id.as_deref() == Some(source_id)
                })
                .cloned())
        }
        async fn find_by_date_range(
            &self,
            _user_id: UserId,
            _start: NaiveDate,
            _end: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(vec![])
        }
        async fn find_planned_before(
            &self,
            _user_id: UserId,
            _before_date: NaiveDate,
        ) -> Result<Vec<Task>, RepositoryError> {
            Ok(vec![])
        }
        async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
            self.tasks.lock().unwrap().insert(task.id, task.clone());
            Ok(())
        }
        async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError> {
            for t in tasks {
                self.save(t).await?;
            }
            Ok(())
        }
        async fn delete(&self, id: TaskId) -> Result<(), RepositoryError> {
            self.tasks.lock().unwrap().remove(&id);
            Ok(())
        }
        async fn delete_stale_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _keep_ids: &[String],
        ) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    struct StubProjectRepo;

    #[async_trait]
    impl ProjectRepository for StubProjectRepo {
        async fn find_by_id(&self, _id: ProjectId) -> Result<Option<Project>, RepositoryError> {
            Ok(None)
        }
        async fn find_by_user(&self, _user_id: UserId) -> Result<Vec<Project>, RepositoryError> {
            Ok(vec![])
        }
        async fn find_by_source(
            &self,
            _user_id: UserId,
            _source: Source,
            _source_key: &str,
        ) -> Result<Option<Project>, RepositoryError> {
            Ok(None)
        }
        async fn save(&self, _project: &Project) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete(&self, _id: ProjectId) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    struct StubSyncRepo;

    #[async_trait]
    impl SyncStatusRepository for StubSyncRepo {
        async fn find_by_user(&self, _user_id: UserId) -> Result<Vec<SyncStatus>, RepositoryError> {
            Ok(vec![])
        }
        async fn upsert(&self, _status: &SyncStatus) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn jira_sync_preserves_delegated_to() {
        let user_id: UserId =
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let task_repo = MiniTaskRepo::default();
        let now = Utc::now();

        // Pre-existing synced task that the user has delegated locally.
        let existing = Task {
            id: Uuid::new_v4(),
            user_id,
            title: "Old title".to_string(),
            description: None,
            notes: Some("local notes".to_string()),
            source: Source::Jira,
            source_id: Some("AP-1".to_string()),
            jira_status: Some("To Do".to_string()),
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            delegated_to: Some("Marie".to_string()),
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            recurrence_id: None,
            occurrence_date: None,
            created_at: now,
            updated_at: now,
        };
        task_repo.save(&existing).await.unwrap();

        let config = JiraConfig {
            project_keys: vec!["AP".to_string()],
            assignees: None,
            my_tasks_only: false,
        };
        let result = sync_jira(
            &StubJiraClient,
            &task_repo,
            &StubProjectRepo,
            &StubSyncRepo,
            user_id,
            &config,
        )
        .await
        .unwrap();
        assert_eq!(result.tasks_updated, 1);

        let after = task_repo
            .find_by_source(user_id, Source::Jira, "AP-1")
            .await
            .unwrap()
            .unwrap();
        // Sync did run and updated Jira-owned fields…
        assert_eq!(after.title, "Synced title");
        assert_eq!(after.assignee.as_deref(), Some("jira.user@example.com"));
        // …but user-owned fields survived.
        assert_eq!(
            after.delegated_to.as_deref(),
            Some("Marie"),
            "delegated_to must survive a Jira resync"
        );
        assert_eq!(after.notes.as_deref(), Some("local notes"));
    }
```

- [ ] **Step 2: Run — must pass**

Run: `cd backend && cargo test -p application jira_sync_preserves_delegated_to`
Expected: PASS (the sync update path doesn't touch the field; this pins it). If it FAILS, sync is clobbering a user-owned field — fix sync, not the test.

- [ ] **Step 3: Commit**

```bash
git add backend/crates/application/src/use_cases/sync.rs
git commit -m "test(application): pin delegated_to survival across Jira sync"
```

---

### Task 5: GraphQL API — field, input, `delegates` query (TDD)

**Files:**
- Modify: `backend/crates/api/src/graphql/types/task.rs` (TaskGql + UpdateTaskInput)
- Modify: `backend/crates/api/src/graphql/mutation.rs` (convert_update_input real mapping)
- Modify: `backend/crates/api/src/graphql/query.rs` (delegates query)
- Modify: `backend/crates/api/src/graphql/tests.rs` (tests + in-memory repo `list_delegates`)

- [ ] **Step 1: Write the failing tests**

In `backend/crates/api/src/graphql/tests.rs`, append:

```rust
#[tokio::test]
async fn update_task_sets_and_clears_delegated_to() {
    let schema = build_test_schema();

    let create = schema
        .execute(r#"mutation { createTask(input: { title: "Delegate me" }) { id } }"#)
        .await;
    assert!(create.errors.is_empty(), "Errors: {:?}", create.errors);
    let task_id = create.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Set
    let set = schema
        .execute(format!(
            r#"mutation {{ updateTask(id: "{}", input: {{ delegatedTo: "Marie" }}) {{ id delegatedTo }} }}"#,
            task_id
        ))
        .await;
    assert!(set.errors.is_empty(), "Errors: {:?}", set.errors);
    assert_eq!(
        set.data.into_json().unwrap()["updateTask"]["delegatedTo"],
        "Marie"
    );

    // Clear with explicit null
    let clear = schema
        .execute(format!(
            r#"mutation {{ updateTask(id: "{}", input: {{ delegatedTo: null }}) {{ id delegatedTo }} }}"#,
            task_id
        ))
        .await;
    assert!(clear.errors.is_empty(), "Errors: {:?}", clear.errors);
    assert!(clear.data.into_json().unwrap()["updateTask"]["delegatedTo"].is_null());
}

#[tokio::test]
async fn delegates_query_returns_learned_names() {
    let schema = build_test_schema();

    for (title, name) in [("T1", "Marie"), ("T2", "Ahmed"), ("T3", "Marie")] {
        let create = schema
            .execute(format!(
                r#"mutation {{ createTask(input: {{ title: "{}" }}) {{ id }} }}"#,
                title
            ))
            .await;
        let id = create.data.into_json().unwrap()["createTask"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let update = schema
            .execute(format!(
                r#"mutation {{ updateTask(id: "{}", input: {{ delegatedTo: "{}" }}) {{ id }} }}"#,
                id, name
            ))
            .await;
        assert!(update.errors.is_empty(), "Errors: {:?}", update.errors);
    }

    let result = schema.execute("{ delegates }").await;
    assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
    let data = result.data.into_json().unwrap();
    assert_eq!(data["delegates"], serde_json::json!(["Ahmed", "Marie"]));
}
```

- [ ] **Step 2: Run — must fail**

Run: `cd backend && cargo test -p api delegated`
Expected: FAIL — `delegatedTo`/`delegates` unknown to the schema.

- [ ] **Step 3: Expose the field on TaskGql**

In `backend/crates/api/src/graphql/types/task.rs`, in `impl TaskGql` after the `assignee` resolver (~line 68):

```rust
    /// Person this task is delegated to. User-owned — preserved across syncs.
    async fn delegated_to(&self) -> Option<&str> {
        self.0.delegated_to.as_deref()
    }
```

- [ ] **Step 4: Add the input field**

Same file, in `UpdateTaskInput` after `estimated_hours_override`:

```rust
    /// Set to a name to delegate, explicit null to clear, omit to leave unchanged.
    pub delegated_to: Option<Option<String>>,
```

- [ ] **Step 5: Map it in `convert_update_input`**

In `backend/crates/api/src/graphql/mutation.rs`, replace the Task-3 placeholder `delegated_to: None,` in the returned literal with:

```rust
        delegated_to: input.delegated_to,
```

- [ ] **Step 6: Add the `delegates` query**

In `backend/crates/api/src/graphql/query.rs`, inside `impl QueryRoot` (after the `tags` resolver is a fine spot):

```rust
    /// Distinct delegate names previously used on the current user's tasks.
    /// Backs the auto-learned suggestion list for the delegation field.
    async fn delegates(&self, ctx: &Context<'_>) -> Result<Vec<String>> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let user_id = ctx.data::<UserId>()?;
        task_repo
            .list_delegates(*user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }
```

- [ ] **Step 7: Implement `list_delegates` on the test in-memory repo**

In `backend/crates/api/src/graphql/tests.rs`, inside `impl TaskRepository for InMemoryTaskRepository` (the trait default returns `vec![]`, which would fail the delegates test):

```rust
    async fn list_delegates(&self, user_id: UserId) -> Result<Vec<String>, RepositoryError> {
        let tasks = self.tasks.lock().unwrap();
        let mut names: Vec<String> = tasks
            .values()
            .filter(|t| t.user_id == user_id)
            .filter_map(|t| t.delegated_to.clone())
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }
```

- [ ] **Step 8: Run — must pass**

Run: `cd backend && cargo test -p api && cargo test`
Expected: all PASS across the workspace.

- [ ] **Step 9: Commit**

```bash
git add backend/crates/api/
git commit -m "feat(api): delegatedTo field on tasks and delegates query"
```

---

### Task 6: Frontend — `useDelegates` hook + TaskEditSheet input (TDD)

**Files:**
- Create: `frontend/src/hooks/use-delegates.ts`
- Modify: `frontend/src/hooks/use-task-edit.ts`
- Modify: `frontend/src/components/task/TaskEditSheet.tsx`
- Modify: `frontend/src/components/task/TaskEditSheet.test.tsx`
- Modify: `frontend/src/graphql/mutations/task.graphql`

- [ ] **Step 1: Write the failing tests**

In `frontend/src/components/task/TaskEditSheet.test.tsx`:

(a) Add the new hook mock next to the existing `vi.mock('@/hooks/use-task-edit', ...)`:

```tsx
vi.mock('@/hooks/use-delegates', () => ({
  useDelegates: () => ({ delegates: ['Ahmed', 'Marie'] }),
}));
```

(b) Add `delegatedTo: null,` to the `BASE_TASK` fixture (TS strict will require it once `FullTask` gains the field — adding it now is part of the red step).

(c) Add tests (in a new `describe('delegation', ...)` block, using the file's existing helpers):

```tsx
describe('delegation', () => {
  beforeEach(() => {
    mockTask = { ...BASE_TASK };
    mockUpdateTask.mockClear();
  });

  it('renders the delegated-to input with learned suggestions', () => {
    renderSheet();
    const input = screen.getByLabelText(/delegated to/i);
    expect(input).toHaveAttribute('list', 'delegate-suggestions');
    const datalist = document.getElementById('delegate-suggestions');
    expect(datalist).not.toBeNull();
    const options = Array.from(datalist!.querySelectorAll('option')).map(o => o.getAttribute('value'));
    expect(options).toEqual(['Ahmed', 'Marie']);
  });

  it('sends delegatedTo on save when a name is entered', async () => {
    renderSheet();
    fireEvent.change(screen.getByLabelText(/delegated to/i), { target: { value: 'Marie' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));
    await waitFor(() => {
      expect(mockUpdateTask).toHaveBeenCalledWith(
        expect.objectContaining({ delegatedTo: 'Marie' })
      );
    });
  });

  it('sends delegatedTo: null on save when the field is emptied', async () => {
    mockTask = { ...BASE_TASK, delegatedTo: 'Marie' };
    renderSheet();
    fireEvent.change(screen.getByLabelText(/delegated to/i), { target: { value: '' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));
    await waitFor(() => {
      expect(mockUpdateTask).toHaveBeenCalledWith(
        expect.objectContaining({ delegatedTo: null })
      );
    });
  });

  it('does not send delegatedTo when unchanged', async () => {
    mockTask = { ...BASE_TASK, delegatedTo: 'Marie', notes: 'x' };
    renderSheet();
    // change something else so save fires an update
    fireEvent.change(screen.getByLabelText('Notes'), { target: { value: 'y' } });
    fireEvent.click(screen.getByTestId('task-sheet-save'));
    await waitFor(() => expect(mockUpdateTask).toHaveBeenCalled());
    expect(mockUpdateTask.mock.calls[0][0]).not.toHaveProperty('delegatedTo');
  });
});
```

- [ ] **Step 2: Run — must fail**

Run: `cd frontend && pnpm test -- TaskEditSheet`
Expected: FAIL — `delegatedTo` not on `FullTask` (TS error) and no such input rendered.

- [ ] **Step 3: Create the `useDelegates` hook**

`frontend/src/hooks/use-delegates.ts`:

```ts
import { useQuery } from 'urql';

const DELEGATES_QUERY = `
  query Delegates {
    delegates
  }
`;

/** Auto-learned list of names previously used in the delegated-to field. */
export function useDelegates() {
  const [result] = useQuery<{ delegates: string[] }>({
    query: DELEGATES_QUERY,
    requestPolicy: 'cache-and-network',
  });
  return { delegates: result.data?.delegates ?? [] };
}
```

- [ ] **Step 4: Extend `use-task-edit.ts`**

- In `FullTask`, after `readonly assignee: string | null;`:

```ts
  readonly delegatedTo: string | null;
```

- In `TASK_QUERY`, add `delegatedTo` on the line after `assignee`.
- In `UPDATE_TASK_MUTATION` selection set, add `delegatedTo` after `notes`.

- [ ] **Step 5: Add the input to `TaskEditSheet.tsx`**

- Import: `import { useDelegates } from '@/hooks/use-delegates';`
- In the component body, after the `useTaskEdit` call: `const { delegates } = useDelegates();`
- Local state, after `plannedDate`:

```tsx
  const [delegatedTo, setDelegatedTo] = useState('');
```

- In the task-load `useEffect`, after `setPlannedDate(...)`:

```tsx
      setDelegatedTo(task.delegatedTo ?? '');
```

- In `handleSave`, after the notes diff block (~line 108):

```tsx
    const newDelegate = delegatedTo.trim() || null;
    if (newDelegate !== (task.delegatedTo ?? null)) {
      perInstanceChanges.delegatedTo = newDelegate;
    }
```

  And add `delegatedTo` to the `useCallback` dependency array.

- In the JSX, inside the "Editable fields" section right after the Planned Date `<div>` (before the `Priority` heading):

```tsx
                    <div>
                      <label htmlFor="task-delegated-to" className="block text-xs font-medium text-gray-700 mb-1">
                        Delegated to
                      </label>
                      <input
                        id="task-delegated-to"
                        type="text"
                        list="delegate-suggestions"
                        value={delegatedTo}
                        onChange={(e) => setDelegatedTo(e.target.value)}
                        placeholder="Nobody — type a name to delegate"
                        className="w-full rounded-md border border-gray-300 px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                      />
                      <datalist id="delegate-suggestions">
                        {delegates.map((name) => (
                          <option key={name} value={name} />
                        ))}
                      </datalist>
                    </div>
```

- [ ] **Step 6: Mirror in the .graphql document**

In `frontend/src/graphql/mutations/task.graphql`, `UpdateTask` selection set — add `delegatedTo` after `notes`.

- [ ] **Step 7: Run — must pass**

Run: `cd frontend && pnpm test -- TaskEditSheet`
Expected: all PASS (including pre-existing sheet tests — the `BASE_TASK` fixture change keeps them compiling).

- [ ] **Step 8: Commit**

```bash
git add frontend/src/hooks/use-delegates.ts frontend/src/hooks/use-task-edit.ts frontend/src/components/task/TaskEditSheet.tsx frontend/src/components/task/TaskEditSheet.test.tsx frontend/src/graphql/mutations/task.graphql
git commit -m "feat(frontend): delegated-to combobox in task edit sheet"
```

---

### Task 7: TaskCard badge + surface wiring (TDD)

**Files:**
- Modify: `frontend/src/components/task/TaskCard.tsx`
- Modify: `frontend/src/components/task/TaskCard.test.tsx`
- Modify: `frontend/src/hooks/use-priority-matrix.ts`, `frontend/src/components/priority/QuadrantColumn.tsx`
- Modify: `frontend/src/hooks/use-triage.ts`, `frontend/src/pages/TriagePage.tsx`

- [ ] **Step 1: Write the failing test**

In `frontend/src/components/task/TaskCard.test.tsx`, add (follow the file's existing render-helper conventions for required props):

```tsx
  it('shows the delegate name when delegatedTo is set', () => {
    renderCard({ delegatedTo: 'Marie' });
    expect(screen.getByText('→ Marie')).toBeInTheDocument();
  });

  it('shows no delegate badge when delegatedTo is absent', () => {
    renderCard({});
    expect(screen.queryByText(/^→ /)).not.toBeInTheDocument();
  });
```

(If the file has no shared `renderCard` helper, render `<TaskCard …requiredProps delegatedTo="Marie" />` directly, copying the required props from an adjacent test.)

- [ ] **Step 2: Run — must fail**

Run: `cd frontend && pnpm test -- TaskCard`
Expected: FAIL — prop doesn't exist / badge not rendered.

- [ ] **Step 3: Implement in `TaskCard.tsx`**

- In `TaskCardProps`, after `readonly assignee?: string | null;` (line 22):

```ts
  readonly delegatedTo?: string | null;
```

- Destructure `delegatedTo,` in the `TaskCard({ ... })` parameter list (after `assignee,`).
- In **both** bottom meta rows — the compact card (after the `{assignee && ...}` span, ~line 156) and the full card (after the `{assignee && ...}` block, ~line 243) — add:

```tsx
          {delegatedTo && (
            <span className="text-xs text-violet-700 truncate" title={`Délégué à ${delegatedTo}`}>
              → {delegatedTo}
            </span>
          )}
```

- [ ] **Step 4: Run — must pass**

Run: `cd frontend && pnpm test -- TaskCard`
Expected: PASS.

- [ ] **Step 5: Wire the data through the two surfaces that already pass `assignee`**

- `frontend/src/hooks/use-priority-matrix.ts`: add `readonly delegatedTo: string | null;` to the task interface (after `assignee`, line 14) and append `delegatedTo` to each of the 4 quadrant field lists (lines 47, 53, 59, 65 — each currently reads `id title status urgency impact deadline assignee source sourceId jiraStatus …`).
- `frontend/src/components/priority/QuadrantColumn.tsx`: add `readonly delegatedTo: string | null;` to its task interface (line 13) and `delegatedTo={task.delegatedTo}` next to both `assignee={task.assignee}` usages (lines 49, 83).
- `frontend/src/hooks/use-triage.ts`: add `readonly delegatedTo: string | null;` to the interface (line 15) and `delegatedTo` to the query selection (after `assignee`, line 38).
- `frontend/src/pages/TriagePage.tsx`: add `delegatedTo={task.delegatedTo}` next to both `assignee={task.assignee}` usages (lines 54, 90).

- [ ] **Step 6: Full frontend verification**

Run: `cd frontend && pnpm test && pnpm build`
Expected: all tests PASS, build clean (strict TS catches any missed interface).

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/task/TaskCard.tsx frontend/src/components/task/TaskCard.test.tsx frontend/src/hooks/use-priority-matrix.ts frontend/src/components/priority/QuadrantColumn.tsx frontend/src/hooks/use-triage.ts frontend/src/pages/TriagePage.tsx
git commit -m "feat(frontend): show delegate badge on task cards"
```

---

### Task 8: Specification updates + final verification

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`
- Modify: `SPEC_TECHNIQUE.md`

- [ ] **Step 1: SPEC_FONCTIONNELLE.md**

Add a subsection in the task-management feature area (match the document's existing heading style and numbering; specs are in French):

```markdown
### Délégation de tâche

Une tâche peut être marquée comme déléguée à une personne via un champ texte libre
« Delegated to » dans le panneau d'édition.

- **Champ purement informatif** : aucune incidence sur la charge, les alertes ou la
  matrice de priorités.
- **Suggestions auto-apprises** : la liste de suggestions est constituée des noms
  déjà utilisés sur les tâches de l'utilisateur (requête `delegates`). Aucune
  gestion de liste dans les paramètres ; tout nouveau nom saisi enrichit la liste.
- **Champ local** : distinct de l'assigné Jira (`assignee`, lecture seule) ; la
  valeur survit aux synchronisations Jira/Excel, comme les notes.
- **Affichage** : le nom du délégué apparaît sur les cartes de tâche (préfixé
  d'une flèche « → ») et dans le panneau d'édition. Vider le champ retire la
  délégation.
```

- [ ] **Step 2: SPEC_TECHNIQUE.md**

Update the matching sections (follow the document's existing structure):

- Tasks table / data model: add `delegated_to TEXT NULL` — personne à qui la tâche est déléguée (texte libre, propriété de l'utilisateur, jamais écrasée par la synchronisation). Migration `008_add_delegated_to.sql`.
- GraphQL API: champ `delegatedTo` sur `Task` ; champ `delegatedTo` sur `UpdateTaskInput` (null explicite = effacer) ; nouvelle requête `delegates: [String!]!` (noms distincts triés, `SELECT DISTINCT delegated_to`).

- [ ] **Step 3: Full-stack verification**

Run: `cd backend && cargo test && cargo clippy -- -D warnings`
Expected: all tests PASS, no clippy warnings.

Run: `cd frontend && pnpm test && pnpm build`
Expected: all tests PASS, clean build.

- [ ] **Step 4: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "docs(spec): document task delegation field"
```

---

## Out of scope (do not add)

Settings UI for names, people/contacts table, delegate filtering/grouping, workload/alert exclusion, "delegated" task status. See spec's YAGNI section.
