# Task Triage & Tracking State Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a triage workflow so users can decide which synced tasks to follow in this app, using drag-and-drop to move tasks between Inbox, Followed, and Dismissed states.

**Architecture:** Add a `tracking_state` field (enum: Inbox/Followed/Dismissed) to the Task domain model. Synced tasks default to Inbox; personally created tasks default to Followed. A new Triage page provides a two-column drag-and-drop interface using @dnd-kit. The Dashboard filters to only show Followed tasks.

**Tech Stack:** Rust (domain enum, SQLite migration), async-graphql (enum + mutation), React + @dnd-kit/core (drag-and-drop UI), urql (GraphQL client)

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `migrations/sqlite/002_add_tracking_state.sql` | Add tracking_state column |
| Modify | `backend/crates/domain/src/types/common.rs` | Add `TrackingState` enum |
| Modify | `backend/crates/domain/src/types/task.rs` | Add `tracking_state` field |
| Modify | `backend/crates/application/src/repositories/task_repository.rs` | Add `tracking_state` to `TaskFilter` |
| Modify | `backend/crates/application/src/use_cases/task_management.rs` | Add `set_tracking_state` use case |
| Modify | `backend/crates/application/src/use_cases/dashboard.rs` | Filter dashboard tasks by `Followed` |
| Modify | `backend/crates/infrastructure/src/database/task_repo.rs` | SQL read/write for tracking_state |
| Modify | `backend/crates/api/src/graphql/types/enums.rs` | Add `TrackingStateGql` enum + conversions |
| Modify | `backend/crates/api/src/graphql/types/task.rs` | Expose `trackingState` field, add to `TaskFilterInput` |
| Modify | `backend/crates/api/src/graphql/mutation.rs` | Add `setTrackingState` + `setTrackingStateBatch` mutations |
| Modify | `backend/crates/api/src/graphql/query.rs:352` | Wire `tracking_state` in filter conversion |
| Create | `frontend/src/hooks/use-triage.ts` | Hook for triage data + mutations |
| Create | `frontend/src/pages/TriagePage.tsx` | Drag-and-drop triage interface |
| Modify | `frontend/src/hooks/use-dashboard.ts` | Add `trackingState` to dashboard query |
| Modify | `frontend/src/pages/DashboardPage.tsx` | Show only Followed tasks, display tracking badge |
| Modify | `frontend/src/components/layout/Sidebar.tsx` | Add Triage nav item |
| Modify | `frontend/src/App.tsx` | Add /triage route |

---

## Chunk 1: Backend — Domain, Application, Infrastructure, API

### Task 1: Database Migration

**Files:**
- Create: `migrations/sqlite/002_add_tracking_state.sql`

- [ ] **Step 1: Write migration SQL**

```sql
-- Add tracking_state column to tasks table.
-- Values: 'inbox' (default for synced tasks), 'followed', 'dismissed'.
ALTER TABLE tasks ADD COLUMN tracking_state TEXT NOT NULL DEFAULT 'inbox'
    CHECK (tracking_state IN ('inbox', 'followed', 'dismissed'));

-- Personal tasks created by the user should be auto-followed.
UPDATE tasks SET tracking_state = 'followed' WHERE source = 'personal';
```

- [ ] **Step 2: Verify migration file exists**

Run: `cat migrations/sqlite/002_add_tracking_state.sql`
Expected: The SQL above is printed.

- [ ] **Step 3: Commit**

```bash
git add migrations/sqlite/002_add_tracking_state.sql
git commit -m "feat: add tracking_state column migration"
```

---

### Task 2: Domain Layer — TrackingState Enum + Task Field

**Files:**
- Modify: `backend/crates/domain/src/types/common.rs` (after `TaskLinkType` enum, around line 95)
- Modify: `backend/crates/domain/src/types/task.rs` (add field after `tags`, line 25)

- [ ] **Step 1: Write failing test for TrackingState parsing**

Add to `backend/crates/domain/src/types/common.rs` at the bottom (in a new `#[cfg(test)] mod tests` block):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_state_display_roundtrip() {
        let states = [TrackingState::Inbox, TrackingState::Followed, TrackingState::Dismissed];
        for state in &states {
            let s = state.to_string();
            let parsed: TrackingState = s.parse().unwrap();
            assert_eq!(&parsed, state);
        }
    }

    #[test]
    fn tracking_state_default_is_inbox() {
        assert_eq!(TrackingState::default(), TrackingState::Inbox);
    }

    #[test]
    fn tracking_state_invalid_string_errors() {
        let result: Result<TrackingState, _> = "invalid".parse();
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test -p domain -- tracking_state 2>&1 | tail -20`
Expected: FAIL — `TrackingState` not found.

- [ ] **Step 3: Add TrackingState enum to common.rs**

Add after the `TaskLinkType` enum (line 95) in `backend/crates/domain/src/types/common.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackingState {
    Inbox,
    Followed,
    Dismissed,
}

impl Default for TrackingState {
    fn default() -> Self {
        Self::Inbox
    }
}

impl std::fmt::Display for TrackingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inbox => write!(f, "inbox"),
            Self::Followed => write!(f, "followed"),
            Self::Dismissed => write!(f, "dismissed"),
        }
    }
}

impl std::str::FromStr for TrackingState {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "inbox" => Ok(Self::Inbox),
            "followed" => Ok(Self::Followed),
            "dismissed" => Ok(Self::Dismissed),
            _ => Err(format!("Invalid tracking state: {}", s)),
        }
    }
}
```

- [ ] **Step 4: Add `tracking_state` field to Task struct**

In `backend/crates/domain/src/types/task.rs`, add after `tags: Vec<TagId>` (line 25):

```rust
    pub tracking_state: TrackingState,
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test -p domain -- tracking_state 2>&1 | tail -20`
Expected: 3 tests PASS.

- [ ] **Step 6: Fix all compile errors across the workspace**

The new `tracking_state` field on `Task` will cause compile errors everywhere a `Task` is constructed. Fix each location by adding `tracking_state: TrackingState::Inbox` (or `TrackingState::Followed` for personal tasks).

Key locations to fix:
- `backend/crates/application/src/use_cases/task_management.rs` — `create_personal_task` should set `TrackingState::Followed`
- `backend/crates/application/src/use_cases/sync.rs` — synced tasks should set `TrackingState::Inbox`
- `backend/crates/application/src/use_cases/dashboard.rs` — test helpers that construct `Task`
- `backend/crates/infrastructure/src/database/task_repo.rs` — `map_task_row` reading from DB
- Any other test files that construct `Task` structs

Run: `cd backend && cargo check 2>&1 | head -50`
Fix each error by adding the field with the appropriate default.

- [ ] **Step 7: Run full domain tests**

Run: `cd backend && cargo test -p domain 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 8: Commit**

Stage all files modified in Step 6 (domain + application + infrastructure + api):

```bash
git add backend/
git commit -m "feat(domain): add TrackingState enum and task field"
```

---

### Task 3: Application Layer — Filter + Use Case

**Files:**
- Modify: `backend/crates/application/src/repositories/task_repository.rs` (add to `TaskFilter`)
- Modify: `backend/crates/application/src/use_cases/task_management.rs` (add `set_tracking_state`)

- [ ] **Step 1: Add `tracking_state` to TaskFilter**

In `backend/crates/application/src/repositories/task_repository.rs`, add to `TaskFilter`:

```rust
    pub tracking_state: Option<Vec<TrackingState>>,
```

Also update `TaskFilter::empty()` to include `tracking_state: None`.

- [ ] **Step 1b: Update InMemoryTaskRepository to filter by tracking_state**

In `backend/crates/application/src/use_cases/task_management.rs`, find the `InMemoryTaskRepository` test helper's `find_by_user` method. Add filtering logic for `tracking_state` alongside the existing `status` filter:

```rust
if let Some(ref states) = filter.tracking_state {
    if !states.contains(&task.tracking_state) {
        return false;
    }
}
```

- [ ] **Step 2: Write failing test for set_tracking_state**

Add to `backend/crates/application/src/use_cases/task_management.rs` in the `#[cfg(test)]` module:

```rust
    #[tokio::test]
    async fn set_tracking_state_updates_task() {
        use domain::types::TrackingState;
        let repo = InMemoryTaskRepository::new();
        let user_id = Uuid::new_v4();
        let today = chrono::Utc::now().date_naive();

        let input = CreateTaskInput {
            title: "Test task".to_string(),
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

        let task = create_personal_task(&repo, user_id, input, today).await.unwrap();
        assert_eq!(task.tracking_state, TrackingState::Followed); // personal = auto-followed

        let updated = set_tracking_state(&repo, task.id, TrackingState::Dismissed).await.unwrap();
        assert_eq!(updated.tracking_state, TrackingState::Dismissed);
    }
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd backend && cargo test -p application -- set_tracking_state 2>&1 | tail -20`
Expected: FAIL — `set_tracking_state` function not found.

- [ ] **Step 4: Implement set_tracking_state use case**

Add to `backend/crates/application/src/use_cases/task_management.rs`:

```rust
/// Update the tracking state of a task (inbox → followed/dismissed).
pub async fn set_tracking_state(
    repo: &dyn TaskRepository,
    task_id: TaskId,
    state: TrackingState,
) -> Result<Task, AppError> {
    let mut task = repo
        .find_by_id(task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {} not found", task_id)))?;

    task.tracking_state = state;
    task.updated_at = chrono::Utc::now();
    repo.save(&task).await?;
    Ok(task)
}

/// Batch-update the tracking state for multiple tasks.
pub async fn set_tracking_state_batch(
    repo: &dyn TaskRepository,
    task_ids: Vec<TaskId>,
    state: TrackingState,
) -> Result<Vec<Task>, AppError> {
    let mut results = Vec::with_capacity(task_ids.len());
    for id in task_ids {
        results.push(set_tracking_state(repo, id, state).await?);
    }
    Ok(results)
}
```

Add required imports at the top of the file: `use domain::types::TrackingState;`

- [ ] **Step 5: Run test to verify it passes**

Run: `cd backend && cargo test -p application -- set_tracking_state 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add backend/crates/application/
git commit -m "feat(application): add tracking_state filter and set_tracking_state use case"
```

---

### Task 4: Application Layer — Dashboard Filters by Followed

**Files:**
- Modify: `backend/crates/application/src/use_cases/dashboard.rs` (line 62-66)

- [ ] **Step 1: Write test verifying the active_filter includes tracking_state**

Add to `backend/crates/application/src/use_cases/dashboard.rs` test module:

```rust
    #[test]
    fn active_filter_includes_followed_tracking_state() {
        // Verify the filter we use for dashboard tasks requires Followed state.
        // The actual filtering is validated in the infrastructure integration test.
        let filter = TaskFilter {
            status: Some(vec![TaskStatus::Todo, TaskStatus::InProgress]),
            tracking_state: Some(vec![TrackingState::Followed]),
            ..TaskFilter::empty()
        };
        assert_eq!(filter.tracking_state, Some(vec![TrackingState::Followed]));
        assert_eq!(filter.status, Some(vec![TaskStatus::Todo, TaskStatus::InProgress]));
    }
```

- [ ] **Step 2: Modify get_daily_dashboard to filter by Followed**

In `backend/crates/application/src/use_cases/dashboard.rs`, change the `active_filter` (lines 62-65) to:

```rust
    let active_filter = TaskFilter {
        status: Some(vec![TaskStatus::Todo, TaskStatus::InProgress]),
        tracking_state: Some(vec![TrackingState::Followed]),
        ..TaskFilter::empty()
    };
```

Add `use domain::types::TrackingState;` to the imports at the top.

- [ ] **Step 3: Verify it compiles**

Run: `cd backend && cargo check 2>&1 | head -20`
Expected: Compiles (may have warnings).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/application/src/use_cases/dashboard.rs
git commit -m "feat(dashboard): filter tasks to only show Followed tracking state"
```

---

### Task 5: Infrastructure Layer — SQL Support for tracking_state

**Files:**
- Modify: `backend/crates/infrastructure/src/database/task_repo.rs`

- [ ] **Step 1: Write failing test for tracking_state persistence**

Add to the test module in `backend/crates/infrastructure/src/database/task_repo.rs`:

```rust
    #[tokio::test]
    async fn save_and_read_tracking_state() {
        let pool = setup_test_db().await;
        let repo = SqliteTaskRepository::new(pool);
        let user_id = Uuid::new_v4();

        let task = Task {
            id: Uuid::new_v4(),
            user_id,
            title: "Tracked task".to_string(),
            description: None,
            source: Source::Jira,
            source_id: Some("SCB-999".to_string()),
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            deadline: None,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
            tags: vec![],
            tracking_state: TrackingState::Inbox,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        repo.save(&task).await.unwrap();

        let loaded = repo.find_by_id(task.id).await.unwrap().unwrap();
        assert_eq!(loaded.tracking_state, TrackingState::Inbox);

        // Filter by tracking state
        let filter = TaskFilter {
            tracking_state: Some(vec![TrackingState::Followed]),
            ..TaskFilter::empty()
        };
        let results = repo.find_by_user(user_id, &filter).await.unwrap();
        assert!(results.is_empty()); // task is Inbox, not Followed
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test -p infrastructure -- save_and_read_tracking_state 2>&1 | tail -20`
Expected: FAIL — tracking_state not in SQL / not mapped.

- [ ] **Step 3: Update map_task_row to read tracking_state**

In the `map_task_row` function, add after the existing field mappings:

```rust
        let tracking_state_str: String = row.get("tracking_state");
        let tracking_state: TrackingState = tracking_state_str
            .parse()
            .unwrap_or(TrackingState::Inbox);
```

And add `tracking_state` to the Task construction.

- [ ] **Step 4: Update save() INSERT OR REPLACE SQL to include tracking_state**

The existing `save()` uses `INSERT OR REPLACE INTO tasks`. Add `tracking_state` to the column list, VALUES placeholders, and `.bind()` chain. The pattern matches the existing code:

```rust
// Add to the column list (after 'impact'):
// ..., impact, tracking_state, created_at, updated_at

// Add one more ? to VALUES

// Add .bind() call:
.bind(task.tracking_state.to_string())
```

Note: `save_batch()` delegates to `self.save()` in a loop, so no changes needed there.

- [ ] **Step 5: Add tracking_state filter to find_by_user**

In `find_by_user()`, add handling for the `tracking_state` filter in the dynamic WHERE clause builder. Use parameterized queries matching the existing pattern with `?` placeholders and `bind_values`:

```rust
if let Some(ref states) = filter.tracking_state {
    if !states.is_empty() {
        let placeholders: Vec<&str> = states.iter().map(|_| "?").collect();
        conditions.push(format!("tracking_state IN ({})", placeholders.join(",")));
        for s in states {
            bind_values.push(s.to_string());
        }
    }
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cd backend && cargo test -p infrastructure -- save_and_read_tracking_state 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 7: Run all infrastructure tests**

Run: `cd backend && cargo test -p infrastructure 2>&1 | tail -15`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
git add backend/crates/infrastructure/
git commit -m "feat(infrastructure): persist and filter tracking_state in SQLite"
```

---

### Task 6: API Layer — GraphQL Enum, Field, Mutations

**Files:**
- Modify: `backend/crates/api/src/graphql/types/enums.rs` (add TrackingStateGql)
- Modify: `backend/crates/api/src/graphql/types/task.rs` (expose field, add to filter)
- Modify: `backend/crates/api/src/graphql/mutation.rs` (add mutations)

- [ ] **Step 1: Add TrackingStateGql enum to enums.rs**

Add to `backend/crates/api/src/graphql/types/enums.rs` after the `TaskLinkTypeGql` block (end of file):

```rust
/// GraphQL enum for task tracking state (triage workflow).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TrackingStateGql {
    Inbox,
    Followed,
    Dismissed,
}

impl From<types::TrackingState> for TrackingStateGql {
    fn from(t: types::TrackingState) -> Self {
        match t {
            types::TrackingState::Inbox => TrackingStateGql::Inbox,
            types::TrackingState::Followed => TrackingStateGql::Followed,
            types::TrackingState::Dismissed => TrackingStateGql::Dismissed,
        }
    }
}

impl From<TrackingStateGql> for types::TrackingState {
    fn from(t: TrackingStateGql) -> Self {
        match t {
            TrackingStateGql::Inbox => types::TrackingState::Inbox,
            TrackingStateGql::Followed => types::TrackingState::Followed,
            TrackingStateGql::Dismissed => types::TrackingState::Dismissed,
        }
    }
}
```

- [ ] **Step 2: Add trackingState field to TaskGql**

In `backend/crates/api/src/graphql/types/task.rs`, add inside `impl TaskGql` after the `tag_ids` method (around line 115):

```rust
    async fn tracking_state(&self) -> TrackingStateGql {
        self.0.tracking_state.into()
    }
```

- [ ] **Step 3: Add tracking_state to TaskFilterInput**

In `backend/crates/api/src/graphql/types/task.rs`, add to `TaskFilterInput`:

```rust
    pub tracking_state: Option<Vec<TrackingStateGql>>,
```

- [ ] **Step 4: Add setTrackingState and setTrackingStateBatch mutations**

In `backend/crates/api/src/graphql/mutation.rs`, add inside `impl MutationRoot` (after the `complete_task` method):

```rust
    /// Set the tracking state of a task (inbox/followed/dismissed).
    async fn set_tracking_state(
        &self,
        ctx: &Context<'_>,
        task_id: ID,
        state: TrackingStateGql,
    ) -> Result<TaskGql> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let id = Uuid::parse_str(&task_id)
            .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))?;

        let task = task_management::set_tracking_state(task_repo.as_ref(), id, state.into())
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(TaskGql(task))
    }

    /// Batch-set the tracking state for multiple tasks.
    async fn set_tracking_state_batch(
        &self,
        ctx: &Context<'_>,
        task_ids: Vec<ID>,
        state: TrackingStateGql,
    ) -> Result<Vec<TaskGql>> {
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let ids: Vec<Uuid> = task_ids
            .into_iter()
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|e| async_graphql::Error::new(format!("Invalid task ID: {}", e)))
            })
            .collect::<Result<Vec<_>>>()?;

        let tasks =
            task_management::set_tracking_state_batch(task_repo.as_ref(), ids, state.into())
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(tasks.into_iter().map(TaskGql).collect())
    }
```

- [ ] **Step 5: Verify everything compiles**

Run: `cd backend && cargo check 2>&1 | head -30`
Expected: Compiles (warnings OK).

- [ ] **Step 6: Run full backend tests**

Run: `cd backend && cargo test 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add backend/crates/api/
git commit -m "feat(api): add trackingState field and setTrackingState mutations"
```

---

### Task 7: API Layer — Wire tracking_state Filter in Query Resolver

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs:352` (in `convert_task_filter` function)

- [ ] **Step 1: Add tracking_state mapping to convert_task_filter**

In `backend/crates/api/src/graphql/query.rs`, find the `convert_task_filter` function (around line 352). Inside the `TaskFilter { ... }` struct literal, add after the existing field mappings:

```rust
        tracking_state: filter.tracking_state.map(|states| {
            states.into_iter().map(|s| s.into()).collect()
        }),
```

- [ ] **Step 2: Verify it compiles**

Run: `cd backend && cargo check 2>&1 | head -20`
Expected: Compiles.

- [ ] **Step 3: Run all backend tests**

Run: `cd backend && cargo test 2>&1 | tail -15`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/api/src/graphql/query.rs
git commit -m "feat(api): wire tracking_state filter in GraphQL query resolver"
```

---

### Task 8: Apply Migration to Development Database

- [ ] **Step 1: Stop the backend server if running**

- [ ] **Step 2: Apply the migration**

Run: `cd backend && sqlite3 aggregated_plan.db < ../migrations/sqlite/002_add_tracking_state.sql`

- [ ] **Step 3: Verify the column exists**

Run: `cd backend && sqlite3 aggregated_plan.db "PRAGMA table_info(tasks);" | grep tracking`
Expected: Output shows `tracking_state|TEXT` column.

- [ ] **Step 4: Verify personal tasks are followed**

Run: `cd backend && sqlite3 aggregated_plan.db "SELECT tracking_state, COUNT(*) FROM tasks GROUP BY tracking_state;"`
Expected: Shows `inbox|N` for synced tasks, `followed|M` for personal tasks (if any).

- [ ] **Step 5: Restart backend and test GraphQL**

Run: `cd backend && cargo run -p api &`

Then test:
```bash
curl -s -X POST http://localhost:3001/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ tasks { edges { node { id title trackingState } } } }"}' | head -c 500
```
Expected: Tasks have `trackingState: "INBOX"` or `"FOLLOWED"`.

- [ ] **Step 6: Commit (no code changes, just verification)**

No commit needed — migration already committed in Task 1.

---

## Chunk 2: Frontend — Triage Page, Dashboard Fix, Navigation

### Task 9: Frontend — Triage Hook

**Files:**
- Create: `frontend/src/hooks/use-triage.ts`

- [ ] **Step 1: Create the triage hook**

```typescript
import { useQuery, useMutation } from 'urql';

export interface TriageTask {
  readonly id: string;
  readonly title: string;
  readonly source: string;
  readonly sourceId: string | null;
  readonly status: string;
  readonly jiraStatus: string | null;
  readonly quadrant: string;
  readonly trackingState: string;
  readonly deadline: string | null;
  readonly assignee: string | null;
  readonly project: { readonly name: string } | null;
}

const TRIAGE_TASKS_QUERY = `
  query TriageTasks($trackingState: [TrackingStateGql!]) {
    tasks(filter: { status: [TODO, IN_PROGRESS], trackingState: $trackingState }) {
      edges {
        node {
          id
          title
          source
          sourceId
          status
          jiraStatus
          quadrant
          trackingState
          deadline
          assignee
          project { name }
        }
      }
      totalCount
    }
  }
`;

const SET_TRACKING_STATE = `
  mutation SetTrackingState($taskId: ID!, $state: TrackingStateGql!) {
    setTrackingState(taskId: $taskId, state: $state) {
      id
      trackingState
    }
  }
`;

const SET_TRACKING_STATE_BATCH = `
  mutation SetTrackingStateBatch($taskIds: [ID!]!, $state: TrackingStateGql!) {
    setTrackingStateBatch(taskIds: $taskIds, state: $state) {
      id
      trackingState
    }
  }
`;

interface TasksResponse {
  tasks: {
    edges: readonly { node: TriageTask }[];
    totalCount: number;
  };
}

export function useTriageTasks() {
  // Fetch inbox and followed tasks (not dismissed)
  const [inboxResult, reexecuteInbox] = useQuery<TasksResponse>({
    query: TRIAGE_TASKS_QUERY,
    variables: { trackingState: ['INBOX'] },
  });

  const [followedResult, reexecuteFollowed] = useQuery<TasksResponse>({
    query: TRIAGE_TASKS_QUERY,
    variables: { trackingState: ['FOLLOWED'] },
  });

  const [, setTrackingState] = useMutation(SET_TRACKING_STATE);
  const [, setTrackingStateBatch] = useMutation(SET_TRACKING_STATE_BATCH);

  const refetch = () => {
    reexecuteInbox({ requestPolicy: 'network-only' });
    reexecuteFollowed({ requestPolicy: 'network-only' });
  };

  const followTask = async (taskId: string) => {
    await setTrackingState({ taskId, state: 'FOLLOWED' });
    refetch();
  };

  const dismissTask = async (taskId: string) => {
    await setTrackingState({ taskId, state: 'DISMISSED' });
    refetch();
  };

  const unfollowTask = async (taskId: string) => {
    await setTrackingState({ taskId, state: 'INBOX' });
    refetch();
  };

  const followAll = async (taskIds: string[]) => {
    await setTrackingStateBatch({ taskIds, state: 'FOLLOWED' });
    refetch();
  };

  const inboxTasks = inboxResult.data?.tasks.edges.map(e => e.node) ?? [];
  const followedTasks = followedResult.data?.tasks.edges.map(e => e.node) ?? [];

  return {
    inboxTasks,
    followedTasks,
    inboxCount: inboxResult.data?.tasks.totalCount ?? 0,
    followedCount: followedResult.data?.tasks.totalCount ?? 0,
    loading: inboxResult.fetching || followedResult.fetching,
    error: inboxResult.error ?? followedResult.error ?? null,
    followTask,
    dismissTask,
    unfollowTask,
    followAll,
    refetch,
  };
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd frontend && pnpm build 2>&1 | tail -10`
Expected: Compiles (the page isn't used yet, but types should be valid).

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/use-triage.ts
git commit -m "feat(frontend): add use-triage hook with follow/dismiss actions"
```

---

### Task 10: Frontend — Triage Page with Drag-and-Drop

**Files:**
- Create: `frontend/src/pages/TriagePage.tsx`

- [ ] **Step 1: Create the triage page with two-column drag-and-drop**

```tsx
import { useState } from 'react';
import {
  DndContext,
  DragOverlay,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragStartEvent,
  type DragEndEvent,
} from '@dnd-kit/core';
import { useDroppable } from '@dnd-kit/core';
import { useDraggable } from '@dnd-kit/core';
import { useTriageTasks, type TriageTask } from '@/hooks/use-triage';

/** A single source color indicator. */
function SourceDot({ source }: { readonly source: string }) {
  const color =
    source === 'JIRA'
      ? 'bg-blue-500'
      : source === 'EXCEL'
        ? 'bg-green-500'
        : 'bg-gray-400';
  return <span className={`inline-block w-2 h-2 rounded-full ${color}`} />;
}

/** Draggable task card for the triage view. */
function DraggableTaskCard({
  task,
  onDismiss,
}: {
  readonly task: TriageTask;
  readonly onDismiss?: () => void;
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
      className="bg-white border border-gray-200 rounded-lg p-3 shadow-sm cursor-grab active:cursor-grabbing hover:border-blue-300 transition-colors"
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5 mb-1">
            <SourceDot source={task.source} />
            {task.sourceId && (
              <span className="text-xs text-gray-400 font-mono">{task.sourceId}</span>
            )}
          </div>
          <p className="text-sm font-medium text-gray-800 truncate">{task.title}</p>
          <div className="flex items-center gap-2 mt-1.5 text-xs text-gray-500">
            <span className="px-1.5 py-0.5 bg-gray-100 rounded text-gray-600">
              {task.status === 'IN_PROGRESS' ? 'In Progress' : 'Todo'}
            </span>
            {task.assignee && <span className="truncate">{task.assignee}</span>}
            {task.deadline && <span>{task.deadline}</span>}
          </div>
          {task.project?.name && (
            <p className="text-xs text-gray-400 mt-1 truncate">{task.project.name}</p>
          )}
        </div>
        {onDismiss && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onDismiss();
            }}
            className="p-1 text-gray-400 hover:text-red-500 transition-colors flex-shrink-0"
            title="Dismiss task"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}

/** Static version of task card for drag overlay. */
function TaskCardOverlay({ task }: { readonly task: TriageTask }) {
  return (
    <div className="bg-white border-2 border-blue-400 rounded-lg p-3 shadow-lg w-80">
      <div className="flex items-center gap-1.5 mb-1">
        <SourceDot source={task.source} />
        {task.sourceId && (
          <span className="text-xs text-gray-400 font-mono">{task.sourceId}</span>
        )}
      </div>
      <p className="text-sm font-medium text-gray-800">{task.title}</p>
    </div>
  );
}

/** Droppable column container. */
function DroppableColumn({
  id,
  title,
  count,
  children,
  accentColor,
  headerAction,
}: {
  readonly id: string;
  readonly title: string;
  readonly count: number;
  readonly children: React.ReactNode;
  readonly accentColor: string;
  readonly headerAction?: React.ReactNode;
}) {
  const { isOver, setNodeRef } = useDroppable({ id });

  return (
    <div
      ref={setNodeRef}
      className={`flex flex-col rounded-lg border-2 transition-colors ${
        isOver ? 'border-blue-400 bg-blue-50/50' : 'border-gray-200 bg-gray-50/50'
      }`}
    >
      <div className={`px-4 py-3 border-b-2 ${accentColor} rounded-t-lg flex items-center justify-between`}>
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">
            {title}
          </h3>
          <span className="text-xs text-gray-500 bg-white/80 px-2 py-0.5 rounded-full">
            {count}
          </span>
        </div>
        {headerAction}
      </div>
      <div className="flex-1 p-3 space-y-2 overflow-y-auto max-h-[calc(100vh-220px)]">
        {children}
      </div>
    </div>
  );
}

export function TriagePage() {
  const {
    inboxTasks,
    followedTasks,
    inboxCount,
    followedCount,
    loading,
    error,
    followTask,
    dismissTask,
    unfollowTask,
    followAll,
  } = useTriageTasks();

  const [activeTask, setActiveTask] = useState<TriageTask | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } })
  );

  const allTasks = [...inboxTasks, ...followedTasks];

  const handleDragStart = (event: DragStartEvent) => {
    const task = allTasks.find(t => t.id === event.active.id);
    setActiveTask(task ?? null);
  };

  const handleDragEnd = (event: DragEndEvent) => {
    setActiveTask(null);
    const { active, over } = event;
    if (!over) return;

    const taskId = active.id as string;
    const targetColumn = over.id as string;

    // Find which column the task is currently in
    const isInInbox = inboxTasks.some(t => t.id === taskId);
    const isInFollowed = followedTasks.some(t => t.id === taskId);

    if (targetColumn === 'followed' && isInInbox) {
      followTask(taskId);
    } else if (targetColumn === 'inbox' && isInFollowed) {
      unfollowTask(taskId);
    }
  };

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load tasks</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  if (loading && inboxTasks.length === 0 && followedTasks.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
          <p className="text-gray-500 text-sm">Loading tasks...</p>
        </div>
      </div>
    );
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
    >
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 h-full">
        {/* Inbox column */}
        <DroppableColumn
          id="inbox"
          title="Inbox"
          count={inboxCount}
          accentColor="border-amber-300 bg-amber-50"
          headerAction={
            inboxTasks.length > 0 ? (
              <button
                onClick={() => followAll(inboxTasks.map(t => t.id))}
                className="text-xs text-blue-600 hover:text-blue-800 font-medium"
              >
                Follow All
              </button>
            ) : undefined
          }
        >
          {inboxTasks.length === 0 ? (
            <p className="text-sm text-gray-400 text-center py-8">
              No new tasks to review
            </p>
          ) : (
            inboxTasks.map(task => (
              <DraggableTaskCard
                key={task.id}
                task={task}
                onDismiss={() => dismissTask(task.id)}
              />
            ))
          )}
        </DroppableColumn>

        {/* Following column */}
        <DroppableColumn
          id="followed"
          title="Following"
          count={followedCount}
          accentColor="border-green-300 bg-green-50"
        >
          {followedTasks.length === 0 ? (
            <p className="text-sm text-gray-400 text-center py-8">
              Drag tasks here to follow them
            </p>
          ) : (
            followedTasks.map(task => (
              <DraggableTaskCard key={task.id} task={task} />
            ))
          )}
        </DroppableColumn>
      </div>

      <DragOverlay>
        {activeTask ? <TaskCardOverlay task={activeTask} /> : null}
      </DragOverlay>
    </DndContext>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd frontend && pnpm build 2>&1 | tail -10`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/TriagePage.tsx
git commit -m "feat(frontend): add Triage page with drag-and-drop inbox/following columns"
```

---

### Task 11: Frontend — Add Route and Navigation

**Files:**
- Modify: `frontend/src/App.tsx` (add route)
- Modify: `frontend/src/components/layout/Sidebar.tsx` (add nav item)

- [ ] **Step 1: Add TriagePage import and route to App.tsx**

In `frontend/src/App.tsx`:

Add import:
```typescript
import { TriagePage } from '@/pages/TriagePage';
```

Add route after the `/dashboard` route:
```tsx
        <Route
          path="/triage"
          element={
            <PageLayout title="Triage">
              <TriagePage />
            </PageLayout>
          }
        />
```

- [ ] **Step 2: Add Triage nav item to Sidebar.tsx**

In `frontend/src/components/layout/Sidebar.tsx`, add to `navItems` array after the Dashboard entry (the Triage page should be the second item since it's part of the core workflow):

```typescript
  {
    path: '/triage',
    label: 'Triage',
    iconPath:
      'M3.75 12h16.5m-16.5 3.75h16.5M3.75 19.5h16.5M5.625 4.5h12.75a1.875 1.875 0 010 3.75H5.625a1.875 1.875 0 010-3.75z',
  },
```

- [ ] **Step 3: Verify it compiles and renders**

Run: `cd frontend && pnpm build 2>&1 | tail -10`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/App.tsx frontend/src/components/layout/Sidebar.tsx
git commit -m "feat(frontend): add Triage route and sidebar navigation"
```

---

### Task 12: Frontend — Update Dashboard to Show Only Followed Tasks

**Files:**
- Modify: `frontend/src/hooks/use-dashboard.ts` (add trackingState to query)
- Modify: `frontend/src/pages/DashboardPage.tsx` (update heading, add badge)

- [ ] **Step 1: Add trackingState to the dashboard GraphQL query**

In `frontend/src/hooks/use-dashboard.ts`, add `trackingState` to the task fields in `DASHBOARD_QUERY` (around line 96, after `source`):

```graphql
        trackingState
```

Also add `trackingState` to the `DashboardTask` interface:

```typescript
  readonly trackingState: string;
```

- [ ] **Step 2: Update DashboardPage heading from "Tasks of the Day" to "Followed Tasks"**

In `frontend/src/pages/DashboardPage.tsx`, change the heading (line 138):

From:
```tsx
                    Tasks of the Day
```
To:
```tsx
                    Followed Tasks
```

- [ ] **Step 3: Verify it compiles**

Run: `cd frontend && pnpm build 2>&1 | tail -10`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/use-dashboard.ts frontend/src/pages/DashboardPage.tsx
git commit -m "feat(frontend): dashboard shows only followed tasks with updated heading"
```

---

### Task 13: End-to-End Verification

- [ ] **Step 1: Start backend and frontend**

```bash
cd backend && cargo run -p api &
cd frontend && pnpm dev &
```

- [ ] **Step 2: Verify the Triage page renders**

Navigate to `http://localhost:3000/triage` in the browser. Should see two columns: Inbox (with tasks) and Following (empty initially).

- [ ] **Step 3: Test drag-and-drop**

Drag a task from Inbox to Following. The task should move. Refresh the page — the task should remain in Following.

- [ ] **Step 4: Verify Dashboard filters correctly**

Navigate to `http://localhost:3000/dashboard`. Should only show tasks that were moved to "Following" on the Triage page. The heading should say "Followed Tasks".

- [ ] **Step 5: Test dismiss**

On the Triage page, click the X button on an Inbox task. It should disappear (moved to Dismissed state).

- [ ] **Step 6: Final commit if any fixes needed**

If any fixes were applied, commit them.

---

### Task 14: Update Specs

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`
- Modify: `SPEC_TECHNIQUE.md`

- [ ] **Step 1: Add triage documentation to SPEC_FONCTIONNELLE.md**

Add a new section describing the triage workflow:
- New tracking states (Inbox, Followed, Dismissed)
- Drag-and-drop triage interface
- Dashboard filters by followed tasks only
- Personal tasks are auto-followed

- [ ] **Step 2: Add technical details to SPEC_TECHNIQUE.md**

Document:
- New `tracking_state` column in tasks table
- New `TrackingState` domain enum
- New GraphQL mutations (`setTrackingState`, `setTrackingStateBatch`)
- New `trackingState` field on Task GraphQL type

- [ ] **Step 3: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "docs: add triage workflow to functional and technical specs"
```
