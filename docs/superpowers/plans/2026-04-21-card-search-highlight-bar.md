# Card Search Highlight Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a JIRA-style global task search bar in the Header. Typing highlights matching `TaskCard`s on the current screen (ring + dim others), a dropdown shows top matches across all tasks, and picking a suggestion opens the existing `TaskEditSheet`.

**Architecture:** Client-side Fuse.js over a lean `searchableTasks` GraphQL projection. A single `SearchProvider` at the app root owns query, matches, and the `openTaskId` that drives one consolidated `TaskEditSheet`. `TaskCard` consumes the provider to apply highlight classes. Keyboard shortcuts `/` and `Cmd/Ctrl+K` focus the input.

**Tech Stack:** Rust (axum, async-graphql, sqlx), React 18 + TypeScript, urql, Tailwind CSS, Fuse.js, Vitest + React Testing Library, Playwright.

**Spec:** `docs/superpowers/specs/2026-04-21-card-search-highlight-bar-design.md`

**Assumptions:**
- You are on the `search` branch (already committed spec).
- Backend: `cd backend && cargo test` runs tests; `cargo run -p api` starts the server on port 3001.
- Frontend: `cd frontend && pnpm install`, `pnpm test`, `pnpm test:e2e`, `pnpm dev` (port 3000).
- File paths are absolute from repo root unless noted.

---

## Task 1: Backend — `SearchableTaskGql` type

**Files:**
- Create: `backend/crates/api/src/graphql/types/searchable_task.rs`
- Modify: `backend/crates/api/src/graphql/types/mod.rs`

- [ ] **Step 1: Create the file with the type and its field resolvers**

Write `backend/crates/api/src/graphql/types/searchable_task.rs`:

```rust
use async_graphql::{Object, ID};
use domain::types::Task;

use super::enums::{SourceGql, TaskStatusGql};

/// Lean task projection for client-side search. Carries pre-resolved project
/// and tag names so the resolver can batch their lookup.
pub struct SearchableTaskGql {
    pub task: Task,
    pub project_name: Option<String>,
    pub tag_names: Vec<String>,
}

#[Object]
impl SearchableTaskGql {
    async fn id(&self) -> ID {
        ID(self.task.id.to_string())
    }

    async fn title(&self) -> &str {
        &self.task.title
    }

    async fn source_id(&self) -> Option<&str> {
        self.task.source_id.as_deref()
    }

    async fn source(&self) -> SourceGql {
        self.task.source.into()
    }

    async fn assignee(&self) -> Option<&str> {
        self.task.assignee.as_deref()
    }

    async fn project_name(&self) -> Option<&str> {
        self.project_name.as_deref()
    }

    async fn tags(&self) -> &[String] {
        &self.tag_names
    }

    async fn description(&self) -> Option<&str> {
        self.task.description.as_deref()
    }

    async fn status(&self) -> TaskStatusGql {
        self.task.status.into()
    }
}
```

- [ ] **Step 2: Register the new module**

Edit `backend/crates/api/src/graphql/types/mod.rs`. Add below the existing `pub mod`/`pub use` lines (alphabetize near `sync`):

```rust
pub mod searchable_task;
// ... existing pub mod lines ...

pub use searchable_task::*;
// ... existing pub use lines ...
```

Concretely, insert `pub mod searchable_task;` after `pub mod sync;` and `pub use searchable_task::*;` after `pub use sync::*;`.

- [ ] **Step 3: Verify it compiles**

Run: `cd backend && cargo check -p api`
Expected: clean build, no warnings about unused code (the type is used in Task 2).

If you get a "struct is never constructed" warning, it's fine — the next task adds the constructor call.

- [ ] **Step 4: Commit**

```bash
git add backend/crates/api/src/graphql/types/searchable_task.rs backend/crates/api/src/graphql/types/mod.rs
git commit -m "feat(api): add SearchableTaskGql type"
```

---

## Task 2: Backend — `searchable_tasks` resolver (TDD: non-dismissed filter)

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs`
- Modify: `backend/crates/api/src/graphql/tests.rs`

- [ ] **Step 1: Write the failing test**

In `backend/crates/api/src/graphql/tests.rs`, add near the other `tasks_query_*` tests:

```rust
#[tokio::test]
async fn searchable_tasks_excludes_dismissed() {
    let schema = build_test_schema();

    // Inbox (default) — included
    let _ = schema
        .execute(r#"mutation { createTask(input: { title: "Inbox task" }) { id } }"#)
        .await;

    // Followed — included
    let followed_res = schema
        .execute(r#"mutation { createTask(input: { title: "Followed task" }) { id } }"#)
        .await;
    let followed_id = followed_res.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = schema
        .execute(&format!(
            r#"mutation {{ updateTrackingState(taskId: "{}", state: FOLLOWED) {{ id }} }}"#,
            followed_id
        ))
        .await;

    // Dismissed — excluded
    let dismissed_res = schema
        .execute(r#"mutation { createTask(input: { title: "Dismissed task" }) { id } }"#)
        .await;
    let dismissed_id = dismissed_res.data.into_json().unwrap()["createTask"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = schema
        .execute(&format!(
            r#"mutation {{ updateTrackingState(taskId: "{}", state: DISMISSED) {{ id }} }}"#,
            dismissed_id
        ))
        .await;

    let result = schema
        .execute(r#"{ searchableTasks { id title } }"#)
        .await;
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

    let data = result.data.into_json().unwrap();
    let titles: Vec<String> = data["searchableTasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap().to_string())
        .collect();

    assert_eq!(titles.len(), 2);
    assert!(titles.contains(&"Inbox task".to_string()));
    assert!(titles.contains(&"Followed task".to_string()));
    assert!(!titles.contains(&"Dismissed task".to_string()));
}
```

> **Note on mutation names:** If `updateTrackingState` is not the exact mutation name in this codebase, grep for it: `grep -rn "tracking_state\|trackingState" backend/crates/api/src/graphql/mutation.rs` and use the real name. The test shape does not depend on it.

- [ ] **Step 2: Run the test — expect FAIL**

Run: `cd backend && cargo test -p api searchable_tasks_excludes_dismissed`
Expected: compile error (`searchableTasks` is not a known field) — that is the "red" state.

- [ ] **Step 3: Implement the resolver**

In `backend/crates/api/src/graphql/query.rs`, add the following new resolver method inside `#[Object] impl QueryRoot`. Place it near the existing `tasks` resolver:

```rust
    /// All non-dismissed tasks for the current user, projected to a lean
    /// payload for client-side fuzzy search. Unpaginated on purpose.
    async fn searchable_tasks(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Vec<crate::graphql::types::SearchableTaskGql>> {
        use std::collections::HashMap;
        use application::repositories::TaskFilter;
        use domain::types::{TrackingState, TagId, ProjectId};

        let user_id = *ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn application::repositories::TaskRepository>>()?;
        let tag_repo = ctx.data::<Arc<dyn application::repositories::TagRepository>>()?;
        let project_repo = ctx.data::<Arc<dyn application::repositories::ProjectRepository>>()?;

        let filter = TaskFilter {
            tracking_state: Some(vec![TrackingState::Inbox, TrackingState::Followed]),
            ..TaskFilter::empty()
        };

        let tasks = task_repo
            .find_by_user(user_id, &filter)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let tags = tag_repo
            .find_by_user(user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let tag_names: HashMap<TagId, String> =
            tags.into_iter().map(|t| (t.id, t.name)).collect();

        let projects = project_repo
            .find_by_user(user_id)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let project_names: HashMap<ProjectId, String> =
            projects.into_iter().map(|p| (p.id, p.name)).collect();

        Ok(tasks
            .into_iter()
            .map(|task| {
                let project_name = task
                    .project_id
                    .and_then(|pid| project_names.get(&pid).cloned());
                let tag_names_vec: Vec<String> = task
                    .tags
                    .iter()
                    .filter_map(|tid| tag_names.get(tid).cloned())
                    .collect();
                crate::graphql::types::SearchableTaskGql {
                    task,
                    project_name,
                    tag_names: tag_names_vec,
                }
            })
            .collect())
    }
```

> **If imports at the top of `query.rs` don't already include** `std::sync::Arc` **and** `domain::types::UserId`**,** they are already imported (per the existing `tasks` resolver). No new top-of-file imports required — inline `use` statements cover the rest.

- [ ] **Step 4: Run the test — expect PASS**

Run: `cd backend && cargo test -p api searchable_tasks_excludes_dismissed`
Expected: 1 passed; 0 failed.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/graphql/query.rs backend/crates/api/src/graphql/tests.rs
git commit -m "feat(api): add searchableTasks resolver excluding dismissed"
```

---

## Task 3: Backend — Projection test (tag names + project name)

**Files:**
- Modify: `backend/crates/api/src/graphql/tests.rs`

- [ ] **Step 1: Write the failing test**

Append to `backend/crates/api/src/graphql/tests.rs`:

```rust
#[tokio::test]
async fn searchable_tasks_resolves_tag_and_project_names() {
    let schema = build_test_schema();

    // Create project
    let project_res = schema
        .execute(
            r#"mutation { createProject(input: { name: "Platform Team" }) { id } }"#,
        )
        .await;
    assert!(project_res.errors.is_empty(), "create project: {:?}", project_res.errors);
    let project_id = project_res.data.into_json().unwrap()["createProject"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create tag
    let tag_res = schema
        .execute(r#"mutation { createTag(input: { name: "backend" }) { id } }"#)
        .await;
    assert!(tag_res.errors.is_empty(), "create tag: {:?}", tag_res.errors);
    let tag_id = tag_res.data.into_json().unwrap()["createTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create task referencing both
    let task_res = schema
        .execute(&format!(
            r#"mutation {{ createTask(input: {{
                title: "Refactor auth middleware",
                projectId: "{}",
                tagIds: ["{}"]
            }}) {{ id }} }}"#,
            project_id, tag_id
        ))
        .await;
    assert!(task_res.errors.is_empty(), "create task: {:?}", task_res.errors);

    let result = schema
        .execute(r#"{ searchableTasks { title projectName tags } }"#)
        .await;
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);

    let data = result.data.into_json().unwrap();
    let first = &data["searchableTasks"][0];
    assert_eq!(first["title"], "Refactor auth middleware");
    assert_eq!(first["projectName"], "Platform Team");
    assert_eq!(first["tags"][0], "backend");
}
```

> **If `createProject` / `createTag` / `tagIds` don't exist with those exact names,** grep the mutation file and adapt the mutation calls. The assertion shape (title, projectName, tags[0]) must not change.

- [ ] **Step 2: Run the test — expect PASS**

Because Task 2 already implemented projection, this should pass immediately:

Run: `cd backend && cargo test -p api searchable_tasks_resolves_tag_and_project_names`
Expected: 1 passed. If it fails, investigate — the projection logic in Task 2 covered it, so a failure means either the mutation names are wrong in the test or the projection has a bug.

- [ ] **Step 3: Run the full backend test suite**

Run: `cd backend && cargo test`
Expected: all existing tests still pass (~100+ tests).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/api/src/graphql/tests.rs
git commit -m "test(api): verify searchableTasks projects tag and project names"
```

---

## Task 4: Frontend — Add Fuse.js dependency

**Files:**
- Modify: `frontend/package.json`

- [ ] **Step 1: Install fuse.js**

Run: `cd frontend && pnpm add fuse.js@^7.0.0`
Expected: `package.json` updated; a lockfile may be created.

- [ ] **Step 2: Verify the dep is in place**

Run: `cd frontend && pnpm list fuse.js`
Expected: prints a version line.

- [ ] **Step 3: Verify type-check still passes**

Run: `cd frontend && pnpm build`
Expected: `tsc -b && vite build` completes.

- [ ] **Step 4: Commit**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml
git commit -m "chore(frontend): add fuse.js for client-side task search"
```

---

## Task 5: Frontend — Fuse config + types

**Files:**
- Create: `frontend/src/lib/search/types.ts`
- Create: `frontend/src/lib/search/fuse-config.ts`

- [ ] **Step 1: Create the shared types file**

Write `frontend/src/lib/search/types.ts`:

```ts
export interface SearchableTask {
  readonly id: string;
  readonly title: string;
  readonly sourceId: string | null;
  readonly source: 'JIRA' | 'EXCEL' | 'OBSIDIAN' | 'PERSONAL' | 'OUTLOOK';
  readonly assignee: string | null;
  readonly projectName: string | null;
  readonly tags: readonly string[];
  readonly description: string | null;
  readonly status: 'TODO' | 'IN_PROGRESS' | 'DONE' | 'BLOCKED';
}
```

- [ ] **Step 2: Create the Fuse config file**

Write `frontend/src/lib/search/fuse-config.ts`:

```ts
import type { IFuseOptions } from 'fuse.js';
import type { SearchableTask } from './types';

export const FUSE_OPTIONS: IFuseOptions<SearchableTask> = {
  keys: [
    { name: 'title',       weight: 0.40 },
    { name: 'sourceId',    weight: 0.25 },
    { name: 'tags',        weight: 0.15 },
    { name: 'projectName', weight: 0.08 },
    { name: 'assignee',    weight: 0.07 },
    { name: 'description', weight: 0.05 },
  ],
  threshold: 0.35,
  ignoreLocation: true,
  includeMatches: true,
  minMatchCharLength: 2,
};

export const MAX_MATCHES = 50;
export const MAX_DROPDOWN_ROWS = 8;
export const MIN_QUERY_LENGTH = 2;
```

- [ ] **Step 3: Verify type-check**

Run: `cd frontend && pnpm build`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/search/types.ts frontend/src/lib/search/fuse-config.ts
git commit -m "feat(frontend): add SearchableTask type and Fuse config"
```

---

## Task 6: Frontend — `useSearchableTasks` urql hook

**Files:**
- Create: `frontend/src/hooks/use-searchable-tasks.ts`

- [ ] **Step 1: Create the hook**

Write `frontend/src/hooks/use-searchable-tasks.ts`:

```ts
import { useQuery } from 'urql';
import type { SearchableTask } from '@/lib/search/types';

const SEARCHABLE_TASKS_QUERY = `
  query SearchableTasks {
    searchableTasks {
      id
      title
      sourceId
      source
      assignee
      projectName
      tags
      description
      status
    }
  }
`;

interface UseSearchableTasksResult {
  readonly tasks: readonly SearchableTask[];
  readonly loading: boolean;
  readonly error: Error | null;
  readonly refetch: () => void;
}

export function useSearchableTasks(): UseSearchableTasksResult {
  const [result, reexecute] = useQuery<{ searchableTasks: SearchableTask[] }>({
    query: SEARCHABLE_TASKS_QUERY,
    requestPolicy: 'cache-and-network',
  });

  return {
    tasks: result.data?.searchableTasks ?? [],
    loading: result.fetching,
    error: (result.error as Error | undefined) ?? null,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
```

- [ ] **Step 2: Verify type-check**

Run: `cd frontend && pnpm build`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/use-searchable-tasks.ts
git commit -m "feat(frontend): add useSearchableTasks hook"
```

---

## Task 7: Frontend — `SearchProvider` with TDD

**Files:**
- Create: `frontend/src/lib/search/SearchProvider.tsx`
- Create: `frontend/src/lib/search/SearchProvider.test.tsx`

- [ ] **Step 1: Write the failing tests**

Write `frontend/src/lib/search/SearchProvider.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, act } from '@testing-library/react';
import { SearchProvider, useSearch } from './SearchProvider';
import type { SearchableTask } from './types';

vi.mock('@/hooks/use-searchable-tasks', () => ({
  useSearchableTasks: () => ({ tasks: FIXTURES, loading: false, error: null, refetch: () => {} }),
}));

// TaskEditSheet is rendered by the provider; mock it to a no-op for isolation
vi.mock('@/components/task/TaskEditSheet', () => ({
  TaskEditSheet: ({ taskId }: { taskId: string | null }) =>
    taskId ? <div data-testid="sheet" data-task-id={taskId} /> : null,
}));

const FIXTURES: SearchableTask[] = [
  { id: '1', title: 'Refactor auth middleware', sourceId: 'PROJ-12', source: 'JIRA',
    assignee: 'alice', projectName: 'Platform', tags: ['backend'],
    description: null, status: 'TODO' },
  { id: '2', title: 'Project planning', sourceId: null, source: 'PERSONAL',
    assignee: null, projectName: null, tags: [],
    description: null, status: 'TODO' },
  { id: '3', title: 'Docs update', sourceId: 'DOCS-4', source: 'JIRA',
    assignee: null, projectName: null, tags: [],
    description: null, status: 'TODO' },
];

function Probe({ spy }: { spy: (ctx: ReturnType<typeof useSearch>) => void }) {
  const ctx = useSearch();
  spy(ctx);
  return null;
}

function renderWithProvider() {
  let ctx!: ReturnType<typeof useSearch>;
  const spy = (c: ReturnType<typeof useSearch>) => { ctx = c; };
  render(
    <SearchProvider>
      <Probe spy={spy} />
    </SearchProvider>
  );
  return { getCtx: () => ctx };
}

describe('SearchProvider', () => {
  it('is inactive for queries shorter than 2 chars', () => {
    const { getCtx } = renderWithProvider();
    act(() => getCtx().setQuery('a'));
    expect(getCtx().highlightActive).toBe(false);
    expect(getCtx().matchedIds.size).toBe(0);
  });

  it('activates and finds matches at >= 2 chars', () => {
    const { getCtx } = renderWithProvider();
    act(() => getCtx().setQuery('auth'));
    expect(getCtx().highlightActive).toBe(true);
    expect(getCtx().matchedIds.has('1')).toBe(true);
    expect(getCtx().matchedIds.has('3')).toBe(false);
  });

  it('ranks Jira-key matches above project-name matches', () => {
    const { getCtx } = renderWithProvider();
    act(() => getCtx().setQuery('PROJ-12'));
    const top = getCtx().matches[0];
    expect(top.item.id).toBe('1');
  });

  it('clearQuery resets state', () => {
    const { getCtx } = renderWithProvider();
    act(() => getCtx().setQuery('auth'));
    expect(getCtx().highlightActive).toBe(true);
    act(() => getCtx().clearQuery());
    expect(getCtx().query).toBe('');
    expect(getCtx().highlightActive).toBe(false);
    expect(getCtx().matchedIds.size).toBe(0);
  });

  it('openTaskInSheet + closeSheet drive openTaskId', () => {
    const { getCtx } = renderWithProvider();
    expect(getCtx().openTaskId).toBeNull();
    act(() => getCtx().openTaskInSheet('1'));
    expect(getCtx().openTaskId).toBe('1');
    act(() => getCtx().closeSheet());
    expect(getCtx().openTaskId).toBeNull();
  });
});
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cd frontend && pnpm test -- SearchProvider`
Expected: all 5 tests fail (module not found).

- [ ] **Step 3: Implement the provider**

Write `frontend/src/lib/search/SearchProvider.tsx`:

```tsx
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import Fuse, { type FuseResult } from 'fuse.js';
import { useSearchableTasks } from '@/hooks/use-searchable-tasks';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
import { FUSE_OPTIONS, MAX_MATCHES, MIN_QUERY_LENGTH } from './fuse-config';
import type { SearchableTask } from './types';

interface SearchContextValue {
  readonly query: string;
  readonly setQuery: (q: string) => void;
  readonly matches: readonly FuseResult<SearchableTask>[];
  readonly matchedIds: ReadonlySet<string>;
  readonly highlightActive: boolean;
  readonly openTaskId: string | null;
  readonly openTaskInSheet: (id: string) => void;
  readonly closeSheet: () => void;
  readonly clearQuery: () => void;
  readonly loading: boolean;
  readonly error: Error | null;
}

const SearchContext = createContext<SearchContextValue | null>(null);

export function useSearch(): SearchContextValue {
  const ctx = useContext(SearchContext);
  if (!ctx) throw new Error('useSearch must be used within a SearchProvider');
  return ctx;
}

export function SearchProvider({ children }: { readonly children: ReactNode }) {
  const { tasks, loading, error, refetch } = useSearchableTasks();
  const [query, setQuery] = useState('');
  const [openTaskId, setOpenTaskId] = useState<string | null>(null);

  // Refetch on window focus.
  useEffect(() => {
    const onFocus = () => refetch();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [refetch]);

  const fuse = useMemo(
    () => new Fuse<SearchableTask>([...tasks], FUSE_OPTIONS),
    [tasks]
  );

  const highlightActive = query.trim().length >= MIN_QUERY_LENGTH && !loading && !error;

  const matches = useMemo<FuseResult<SearchableTask>[]>(
    () => (highlightActive ? fuse.search(query).slice(0, MAX_MATCHES) : []),
    [fuse, query, highlightActive]
  );

  const matchedIds = useMemo<ReadonlySet<string>>(
    () => new Set(matches.map((m) => m.item.id)),
    [matches]
  );

  const clearQuery = useCallback(() => setQuery(''), []);
  const openTaskInSheet = useCallback((id: string) => setOpenTaskId(id), []);
  const closeSheet = useCallback(() => setOpenTaskId(null), []);

  const value: SearchContextValue = {
    query,
    setQuery,
    matches,
    matchedIds,
    highlightActive,
    openTaskId,
    openTaskInSheet,
    closeSheet,
    clearQuery,
    loading,
    error,
  };

  return (
    <SearchContext.Provider value={value}>
      {children}
      <TaskEditSheet taskId={openTaskId} onClose={closeSheet} onUpdated={refetch} />
    </SearchContext.Provider>
  );
}
```

- [ ] **Step 4: Run tests — expect PASS**

Run: `cd frontend && pnpm test -- SearchProvider`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/search/SearchProvider.tsx frontend/src/lib/search/SearchProvider.test.tsx
git commit -m "feat(frontend): add SearchProvider with Fuse.js-backed task search"
```

---

## Task 8: Frontend — Wrap app with `SearchProvider`

**Files:**
- Modify: `frontend/src/App.tsx`

- [ ] **Step 1: Wrap `<Routes>` with `<SearchProvider>`**

In `frontend/src/App.tsx`:
- Add import near the other `@/` imports: `import { SearchProvider } from '@/lib/search/SearchProvider';`
- Wrap the `<Routes>` element:

```tsx
export function App() {
  return (
    <BrowserRouter>
      <SearchProvider>
        <Routes>
          {/* existing routes unchanged */}
        </Routes>
      </SearchProvider>
    </BrowserRouter>
  );
}
```

- [ ] **Step 2: Start the backend + frontend and smoke-check**

Terminal 1: `cd backend && cargo run -p api`
Terminal 2: `cd frontend && pnpm dev`

Open `http://localhost:3000/dashboard` in a browser. Expected:
- No console errors about `useSearch` / provider missing.
- The existing dashboard renders as before (search bar not yet added in Header).
- If DevTools → Network: `searchableTasks` GraphQL query is fired once on load.

Stop both servers after verification.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(frontend): mount SearchProvider at app root"
```

---

## Task 9: Frontend — `SuggestionDropdown` component (TDD: rendering + interactions)

**Files:**
- Create: `frontend/src/components/search/SuggestionDropdown.tsx`
- Create: `frontend/src/components/search/SuggestionDropdown.test.tsx`

*(SuggestionDropdown comes before HeaderSearchBar because HeaderSearchBar imports it.)*

- [ ] **Step 1: Write the failing tests**

Unit-test the dropdown by mocking `useSearch` directly — tests focus on rendering and interactions, not Fuse internals.

Write `frontend/src/components/search/SuggestionDropdown.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { FuseResult } from 'fuse.js';
import { SuggestionDropdown } from './SuggestionDropdown';
import type { SearchableTask } from '@/lib/search/types';

interface MockCtx {
  query: string;
  matches: FuseResult<SearchableTask>[];
  matchedIds: ReadonlySet<string>;
  highlightActive: boolean;
  openTaskId: string | null;
  openTaskInSheet: (id: string) => void;
  closeSheet: () => void;
  clearQuery: () => void;
  setQuery: (q: string) => void;
  loading: boolean;
  error: Error | null;
}

let ctx: MockCtx;
vi.mock('@/lib/search/SearchProvider', () => ({
  useSearch: () => ctx,
}));

function task(id: string, title: string, extras: Partial<SearchableTask> = {}): SearchableTask {
  return {
    id,
    title,
    sourceId: null,
    source: 'JIRA',
    assignee: null,
    projectName: null,
    tags: [],
    description: null,
    status: 'TODO',
    ...extras,
  };
}

function result(t: SearchableTask, titleIndices: readonly (readonly [number, number])[] = []): FuseResult<SearchableTask> {
  return {
    item: t,
    refIndex: 0,
    matches: titleIndices.length
      ? [{ key: 'title', indices: titleIndices, value: t.title }]
      : [],
  };
}

const openSpy = vi.fn();
const clearSpy = vi.fn();

beforeEach(() => {
  openSpy.mockClear();
  clearSpy.mockClear();
  ctx = {
    query: 'auth',
    matches: [],
    matchedIds: new Set(),
    highlightActive: true,
    openTaskId: null,
    openTaskInSheet: openSpy,
    closeSheet: vi.fn(),
    clearQuery: clearSpy,
    setQuery: vi.fn(),
    loading: false,
    error: null,
  };
});

describe('SuggestionDropdown', () => {
  it('renders each match as a listbox option', () => {
    ctx.matches = [
      result(task('1', 'Refactor auth middleware'), [[9, 12]]),
      result(task('2', 'Write auth tests'), [[6, 9]]),
    ];
    render(<SuggestionDropdown listboxId="lb" />);
    expect(screen.getAllByRole('option')).toHaveLength(2);
  });

  it('shows an empty state including the query when there are no matches', () => {
    ctx.query = 'zzzzz';
    ctx.matches = [];
    render(<SuggestionDropdown listboxId="lb" />);
    expect(screen.getByText(/No tasks match/i).textContent).toContain('zzzzz');
  });

  it('clicking a row opens the task and clears the query', () => {
    ctx.matches = [result(task('1', 'Auth work'))];
    render(<SuggestionDropdown listboxId="lb" />);
    fireEvent.mouseDown(screen.getAllByRole('option')[0]);
    expect(openSpy).toHaveBeenCalledWith('1');
    expect(clearSpy).toHaveBeenCalled();
  });

  it('ArrowDown on the listbox moves the active option', () => {
    ctx.matches = [
      result(task('1', 'Auth A')),
      result(task('2', 'Auth B')),
    ];
    render(<SuggestionDropdown listboxId="lb" />);
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'ArrowDown' });
    const options = screen.getAllByRole('option');
    expect(options[1]).toHaveAttribute('aria-selected', 'true');
  });

  it('bolds matched character ranges in the title', () => {
    ctx.matches = [result(task('1', 'Refactor auth middleware'), [[9, 12]])];
    render(<SuggestionDropdown listboxId="lb" />);
    expect(screen.getByText('auth').tagName).toBe('STRONG');
  });
});
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cd frontend && pnpm test -- SuggestionDropdown`
Expected: module-not-found failures (the component doesn't exist yet).

- [ ] **Step 3: Implement the dropdown**

Write `frontend/src/components/search/SuggestionDropdown.tsx`:

```tsx
import { useEffect, useRef, useState } from 'react';
import type { FuseResultMatch } from 'fuse.js';
import { useSearch } from '@/lib/search/SearchProvider';
import { MAX_DROPDOWN_ROWS } from '@/lib/search/fuse-config';

interface Props {
  readonly listboxId: string;
}

function renderHighlightedTitle(
  title: string,
  matchIndices: readonly (readonly [number, number])[] | undefined
) {
  if (!matchIndices || matchIndices.length === 0) return title;
  const out: React.ReactNode[] = [];
  let cursor = 0;
  for (const [start, end] of matchIndices) {
    if (start > cursor) out.push(title.slice(cursor, start));
    out.push(<strong key={`${start}-${end}`}>{title.slice(start, end + 1)}</strong>);
    cursor = end + 1;
  }
  if (cursor < title.length) out.push(title.slice(cursor));
  return <>{out}</>;
}

function titleMatchIndices(matches: readonly FuseResultMatch[] | undefined) {
  return matches?.find((m) => m.key === 'title')?.indices;
}

const SOURCE_ICON: Record<string, string> = {
  JIRA: '🧩',
  EXCEL: '📊',
  OBSIDIAN: '🗒️',
  PERSONAL: '📝',
  OUTLOOK: '📅',
};

export function SuggestionDropdown({ listboxId }: Props) {
  const { matches, openTaskInSheet, clearQuery, query } = useSearch();
  const [activeIndex, setActiveIndex] = useState(0);
  const ref = useRef<HTMLUListElement>(null);

  useEffect(() => {
    setActiveIndex(0);
  }, [matches]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (matches.length === 0) return;
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((i) => Math.min(i + 1, matches.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const picked = matches[activeIndex];
        if (picked) {
          openTaskInSheet(picked.item.id);
          clearQuery();
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [matches, activeIndex, openTaskInSheet, clearQuery]);

  if (matches.length === 0) {
    return (
      <div
        role="listbox"
        id={listboxId}
        className="absolute z-30 mt-1 w-full rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-500 shadow-lg"
      >
        No tasks match &ldquo;{query}&rdquo;
      </div>
    );
  }

  return (
    <ul
      ref={ref}
      role="listbox"
      id={listboxId}
      className="absolute z-30 mt-1 w-full overflow-y-auto rounded-md border border-gray-200 bg-white shadow-lg"
      style={{ maxHeight: `${MAX_DROPDOWN_ROWS * 3.25}rem` }}
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          setActiveIndex((i) => Math.min(i + 1, matches.length - 1));
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          setActiveIndex((i) => Math.max(i - 1, 0));
        }
      }}
    >
      {matches.map((m, i) => {
        const { item } = m;
        const active = i === activeIndex;
        const meta = [item.sourceId, item.projectName, item.assignee]
          .filter(Boolean)
          .join(' · ');
        return (
          <li
            key={item.id}
            role="option"
            aria-selected={active}
            onMouseDown={() => {
              openTaskInSheet(item.id);
              clearQuery();
            }}
            onMouseEnter={() => setActiveIndex(i)}
            className={
              'flex cursor-pointer gap-2 px-3 py-2 text-sm ' +
              (active ? 'bg-blue-50' : 'hover:bg-gray-50')
            }
          >
            <span className="pt-0.5">{SOURCE_ICON[item.source] ?? '•'}</span>
            <div className="min-w-0 flex-1">
              <div className="truncate text-gray-900">
                {renderHighlightedTitle(item.title, titleMatchIndices(m.matches))}
              </div>
              {meta && (
                <div className="truncate text-xs text-gray-500">{meta}</div>
              )}
            </div>
          </li>
        );
      })}
    </ul>
  );
}
```

- [ ] **Step 4: Run tests — expect PASS**

Run: `cd frontend && pnpm test -- SuggestionDropdown`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/search/SuggestionDropdown.tsx frontend/src/components/search/SuggestionDropdown.test.tsx
git commit -m "feat(frontend): add SuggestionDropdown with keyboard navigation"
```

---

## Task 10: Frontend — `HeaderSearchBar` component (TDD: shortcuts + input)

**Files:**
- Create: `frontend/src/components/search/HeaderSearchBar.tsx`
- Create: `frontend/src/components/search/HeaderSearchBar.test.tsx`

- [ ] **Step 1: Write the failing tests**

Write `frontend/src/components/search/HeaderSearchBar.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { HeaderSearchBar } from './HeaderSearchBar';
import { SearchProvider } from '@/lib/search/SearchProvider';

vi.mock('@/hooks/use-searchable-tasks', () => ({
  useSearchableTasks: () => ({ tasks: [], loading: false, error: null, refetch: () => {} }),
}));
vi.mock('@/components/task/TaskEditSheet', () => ({ TaskEditSheet: () => null }));

function renderBar() {
  return render(
    <SearchProvider>
      <HeaderSearchBar />
    </SearchProvider>
  );
}

describe('HeaderSearchBar', () => {
  it('renders the input with placeholder', () => {
    renderBar();
    expect(screen.getByRole('combobox')).toBeInTheDocument();
  });

  it('focuses the input when "/" is pressed on the document body', () => {
    renderBar();
    const input = screen.getByRole('combobox') as HTMLInputElement;
    expect(document.activeElement).not.toBe(input);
    fireEvent.keyDown(window, { key: '/' });
    expect(document.activeElement).toBe(input);
  });

  it('focuses the input when Cmd+K is pressed', () => {
    renderBar();
    const input = screen.getByRole('combobox') as HTMLInputElement;
    fireEvent.keyDown(window, { key: 'k', metaKey: true });
    expect(document.activeElement).toBe(input);
  });

  it('ignores "/" while a textarea is focused', () => {
    const { container } = renderBar();
    const textarea = document.createElement('textarea');
    container.appendChild(textarea);
    textarea.focus();
    fireEvent.keyDown(window, { key: '/' });
    expect(document.activeElement).toBe(textarea);
  });

  it('Escape clears the query and blurs the input', () => {
    renderBar();
    const input = screen.getByRole('combobox') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'auth' } });
    expect(input.value).toBe('auth');
    input.focus();
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(input.value).toBe('');
    expect(document.activeElement).not.toBe(input);
  });
});
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cd frontend && pnpm test -- HeaderSearchBar`
Expected: compile errors (module not found) for all tests.

- [ ] **Step 3: Implement the component**

Write `frontend/src/components/search/HeaderSearchBar.tsx`:

```tsx
import { useEffect, useId, useRef, useState } from 'react';
import { useSearch } from '@/lib/search/SearchProvider';
import { SuggestionDropdown } from './SuggestionDropdown';

function isTypingTarget(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA') return true;
  return (el as HTMLElement).isContentEditable === true;
}

export function HeaderSearchBar() {
  const { query, setQuery, clearQuery, highlightActive, loading, error } = useSearch();
  const inputRef = useRef<HTMLInputElement>(null);
  const [isFocused, setIsFocused] = useState(false);
  const listboxId = useId();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const input = inputRef.current;
      if (!input) return;

      // Cmd/Ctrl+K — focus unconditionally
      if (e.key.toLowerCase() === 'k' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        input.focus();
        return;
      }

      // "/" — focus, but only when we aren't already typing somewhere else
      if (e.key === '/' && !isTypingTarget(document.activeElement)) {
        e.preventDefault();
        input.focus();
        return;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const placeholder = error
    ? 'Search unavailable — retry'
    : loading
      ? 'Indexing tasks…'
      : 'Search tasks   /';

  const showDropdown = highlightActive && isFocused;

  return (
    <div className="relative w-80">
      <input
        ref={inputRef}
        type="search"
        role="combobox"
        aria-expanded={showDropdown}
        aria-controls={listboxId}
        aria-autocomplete="list"
        value={query}
        placeholder={placeholder}
        onChange={(e) => setQuery(e.target.value)}
        onFocus={() => setIsFocused(true)}
        onBlur={() => {
          // Delay so a click inside the dropdown still registers
          setTimeout(() => setIsFocused(false), 150);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            clearQuery();
            inputRef.current?.blur();
          }
        }}
        className="w-full rounded-md border border-gray-300 bg-white px-3 py-1.5 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:bg-gray-100 disabled:cursor-not-allowed"
        disabled={!!error}
      />
      {query.length > 0 && !error && (
        <button
          type="button"
          aria-label="Clear search"
          onClick={() => {
            clearQuery();
            inputRef.current?.focus();
          }}
          className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-700"
        >
          ×
        </button>
      )}
      {showDropdown && <SuggestionDropdown listboxId={listboxId} />}
    </div>
  );
}
```

- [ ] **Step 4: Run tests — expect PASS**

Run: `cd frontend && pnpm test -- HeaderSearchBar`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/search/HeaderSearchBar.tsx frontend/src/components/search/HeaderSearchBar.test.tsx
git commit -m "feat(frontend): add HeaderSearchBar with / and Cmd+K shortcuts"
```

---
## Task 11: Frontend — Mount `HeaderSearchBar` inside `Header`

**Files:**
- Modify: `frontend/src/components/layout/Header.tsx`

- [ ] **Step 1: Insert the bar**

Replace the contents of `frontend/src/components/layout/Header.tsx` with:

```tsx
import { HeaderSearchBar } from '@/components/search/HeaderSearchBar';

interface HeaderProps {
  readonly title: string;
}

export function Header({ title }: HeaderProps) {
  return (
    <header className="flex items-center justify-between bg-white border-b border-gray-200 px-6 py-4">
      <h2 className="text-lg font-semibold text-gray-800">{title}</h2>
      <HeaderSearchBar />
    </header>
  );
}
```

- [ ] **Step 2: Smoke-check in the browser**

Terminal 1: `cd backend && cargo run -p api`
Terminal 2: `cd frontend && pnpm dev`

Open `http://localhost:3000/dashboard`. Expected:
- Search input visible on the right of the Header.
- Press `/` outside any input → input focused.
- Press `Cmd/Ctrl+K` → input focused.
- Type 2 chars of any known task title → dropdown appears with suggestions.
- Click a suggestion → `TaskEditSheet` slides in on the right.

Stop servers after verification.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/layout/Header.tsx
git commit -m "feat(frontend): mount HeaderSearchBar in Header"
```

---

## Task 12: Frontend — `TaskCard` highlight classes (TDD)

**Files:**
- Modify: `frontend/src/components/task/TaskCard.tsx`
- Create: `frontend/src/components/task/TaskCard.test.tsx`

- [ ] **Step 1: Write the failing tests**

Write `frontend/src/components/task/TaskCard.test.tsx`:

Mock `useSearch` directly so tests don't have to drive a full `SearchProvider`.

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/react';
import { TaskCard, type TaskCardProps } from './TaskCard';

interface MockCtx {
  query: string;
  matches: [];
  matchedIds: ReadonlySet<string>;
  highlightActive: boolean;
  openTaskId: string | null;
  openTaskInSheet: (id: string) => void;
  closeSheet: () => void;
  clearQuery: () => void;
  setQuery: (q: string) => void;
  loading: boolean;
  error: Error | null;
}

let ctx: MockCtx;
vi.mock('@/lib/search/SearchProvider', () => ({
  useSearch: () => ctx,
}));

const TASK: TaskCardProps['task'] = {
  // Minimum fields the component reads. Adjust if TaskCard's prop type
  // requires more — do not remove `id`.
  id: '1',
  title: 'Example task',
  source: 'JIRA',
  sourceId: 'PROJ-1',
  status: 'TODO',
  urgency: 'MEDIUM',
  impact: 'MEDIUM',
  projectName: null,
  assignee: null,
  tags: [],
} as unknown as TaskCardProps['task'];

beforeEach(() => {
  ctx = {
    query: '',
    matches: [],
    matchedIds: new Set(),
    highlightActive: false,
    openTaskId: null,
    openTaskInSheet: vi.fn(),
    closeSheet: vi.fn(),
    clearQuery: vi.fn(),
    setQuery: vi.fn(),
    loading: false,
    error: null,
  };
});

describe('TaskCard highlight', () => {
  it('renders without ring or dim when search is inactive', () => {
    const { container } = render(<TaskCard task={TASK} />);
    const root = container.querySelector('[data-testid="task-card-root"]');
    expect(root?.className).not.toMatch(/ring-/);
    expect(root?.className).not.toMatch(/opacity-/);
  });

  it('adds ring classes when the task matches', () => {
    ctx.highlightActive = true;
    ctx.matchedIds = new Set(['1']);
    const { container } = render(<TaskCard task={TASK} />);
    const root = container.querySelector('[data-testid="task-card-root"]');
    expect(root?.className).toMatch(/ring-2/);
    expect(root?.className).toMatch(/ring-blue-500/);
  });

  it('adds dim classes when search is active and the task does NOT match', () => {
    ctx.highlightActive = true;
    ctx.matchedIds = new Set(['2']); // some other id
    const { container } = render(<TaskCard task={TASK} />);
    const root = container.querySelector('[data-testid="task-card-root"]');
    expect(root?.className).toMatch(/opacity-40/);
  });
});
```

> If the `TaskCardProps` type or the set of required fields differs from the fixture above, open `TaskCard.tsx` and copy the real prop shape — the fixture must satisfy `TaskCardProps['task']`. Do not change the assertions.

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cd frontend && pnpm test -- TaskCard.test`
Expected: the 3 tests fail because the root has no highlight classes, and likely also because the root lacks the test id.

- [ ] **Step 3: Modify `TaskCard` to consume the search context and apply highlight classes**

Open `frontend/src/components/task/TaskCard.tsx`. At the top:

```tsx
import { useSearch } from '@/lib/search/SearchProvider';
```

Inside the component body, before the return:

```tsx
const { highlightActive, matchedIds } = useSearch();
const isMatch = matchedIds.has(task.id);
const highlightClasses = !highlightActive
  ? ''
  : isMatch
    ? 'ring-2 ring-blue-500 ring-offset-2'
    : 'opacity-40 grayscale-[30%]';
```

Find the root element (the outer `<div>` or `<article>` that has the base Tailwind classes like `bg-white rounded-lg border`). Two edits on that element:

1. Add `data-testid="task-card-root"`.
2. Append `highlightClasses` to its `className`. If the class list is already built with a template string, interpolate `${highlightClasses}`. If it uses an inline concatenation pattern already present in the file, follow that. A minimal safe transform:

```tsx
// before
<div className="bg-white rounded-lg border ...other...">
// after
<div
  data-testid="task-card-root"
  className={`bg-white rounded-lg border ...other... ${highlightClasses}`}
>
```

Do not restructure or extract any other classes — a surgical edit only.

> The `compact` variant in this file (used in the dashboard) has a different root className. Apply the same two edits to that variant's root element as well, so highlighting works on the dashboard cards.

- [ ] **Step 4: Run tests — expect PASS**

Run: `cd frontend && pnpm test -- TaskCard.test`
Expected: 3 passed.

- [ ] **Step 5: Run the full frontend test suite**

Run: `cd frontend && pnpm test`
Expected: everything still green.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/task/TaskCard.tsx frontend/src/components/task/TaskCard.test.tsx
git commit -m "feat(frontend): highlight TaskCards based on SearchProvider state"
```

---

## Task 13: Frontend — Consolidate `TaskEditSheet` into `SearchProvider`

**Files:**
- Modify: `frontend/src/pages/DashboardPage.tsx`
- Modify: `frontend/src/pages/PriorityMatrixPage.tsx`
- Modify: `frontend/src/pages/TriagePage.tsx`

Rationale: the provider already renders one `<TaskEditSheet>` (Task 7). Per-page instances now produce two sheets; we need to remove the per-page ones and redirect their click handlers to `openTaskInSheet` from the context.

- [ ] **Step 1: Update `DashboardPage.tsx`**

- Remove:
  - The `useState<string | null>(null)` for `editingTaskId` and the `handleSheetClose` callback.
  - The `<TaskEditSheet taskId={editingTaskId} onClose={handleSheetClose} />` line.
  - The import of `TaskEditSheet`.
- Add:
  - `import { useSearch } from '@/lib/search/SearchProvider';`
  - Inside the component: `const { openTaskInSheet } = useSearch();`
- Replace every `setEditingTaskId(x)` with `openTaskInSheet(x)`, and replace `setEditingTaskId` passed as a prop with `openTaskInSheet`.
- Any code that did `setEditingTaskId(null)` (e.g. on drag start) should be deleted — the provider closes the sheet via its own `closeSheet`; the page no longer tracks sheet state.

- [ ] **Step 2: Update `PriorityMatrixPage.tsx`**

Same pattern:
- Delete `editingTaskId` state, `handleEdit` callback, `TaskEditSheet` instance, and `TaskEditSheet` import.
- Use `useSearch().openTaskInSheet` in place of `setEditingTaskId`.
- `onDragStartExternal={() => setEditingTaskId(null)}` → `onDragStartExternal={() => { /* sheet state is owned by SearchProvider */ }}` (or simply remove the prop and its consumer if trivial).

- [ ] **Step 3: Update `TriagePage.tsx`**

Same pattern:
- Delete `editingTaskId` state, `TaskEditSheet` instance + import.
- In `handleDragStart`, remove `setEditingTaskId(null)`.
- `onEdit={(id) => setEditingTaskId(id)}` → `onEdit={openTaskInSheet}`.

- [ ] **Step 4: Smoke-check**

Start backend + frontend. For each page (Dashboard, Priority, Triage):
- Click a task card → the edit sheet opens (only one instance).
- Close the sheet → it closes.
- Press `/` then type → dropdown appears; click a suggestion → sheet opens with correct task.

- [ ] **Step 5: Run test suites**

- `cd frontend && pnpm test` — all green.
- `cd backend && cargo test` — all green.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/DashboardPage.tsx frontend/src/pages/PriorityMatrixPage.tsx frontend/src/pages/TriagePage.tsx
git commit -m "refactor(frontend): consolidate TaskEditSheet into SearchProvider"
```

---

## Task 14: Frontend — Playwright E2E happy path

**Files:**
- Modify: `frontend/e2e/smoke.spec.ts`

- [ ] **Step 1: Add the E2E test**

Append to `frontend/e2e/smoke.spec.ts`:

```ts
test('search bar: "/" focuses input, typing shows suggestions, click opens edit sheet', async ({ page }) => {
  await page.goto('/dashboard');

  // Wait for at least one task card to render
  const firstCard = page.locator('[data-testid="task-card-root"]').first();
  await expect(firstCard).toBeVisible();

  // Grab a known title from a visible card
  const title = (await firstCard.textContent())?.trim().split('\n')[0] ?? '';
  expect(title.length).toBeGreaterThan(2);
  const needle = title.slice(0, 3);

  // "/" focuses the search input
  await page.keyboard.press('/');
  const input = page.getByRole('combobox');
  await expect(input).toBeFocused();

  // Typing shows the dropdown listbox
  await input.fill(needle);
  const listbox = page.getByRole('listbox');
  await expect(listbox).toBeVisible();

  // Picking the first suggestion opens the edit sheet
  await page.getByRole('option').first().click();
  await expect(page.getByRole('dialog').or(page.locator('[data-sheet-open="true"]'))).toBeVisible({ timeout: 2000 });
});
```

> If `TaskEditSheet` does not expose `role="dialog"` or `data-sheet-open`, open the component and verify how it can be asserted (often by a visible heading like "Edit task"). Adjust the final `expect` accordingly — the **intent** of the assertion ("edit sheet is visible") must not change.

- [ ] **Step 2: Run E2E**

Ensure backend is running (`cargo run -p api`), then:

Run: `cd frontend && pnpm test:e2e -- smoke.spec.ts`
Expected: smoke test + new search test all pass.

- [ ] **Step 3: Commit**

```bash
git add frontend/e2e/smoke.spec.ts
git commit -m "test(frontend): E2E for search bar happy path"
```

---

## Task 15: Docs — Update functional and technical specs

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`
- Modify: `SPEC_TECHNIQUE.md`

- [ ] **Step 1: Add a "Recherche globale" section to `SPEC_FONCTIONNELLE.md`**

Insert a new section (French) describing:
- A search bar in the top Header, always visible.
- Shortcuts `/` and `Cmd/Ctrl+K` to focus; `Esc` to clear.
- Typing ≥ 2 characters highlights matching tasks on the current screen (ring blue + dim others) and shows a dropdown of the top matches.
- Picking a suggestion opens the existing task edit sheet; no screen navigation.
- Fuzzy matching across title, Jira key, tags, project, assignee, description — tasks only; dismissed tasks excluded.

Place this under an existing section that covers UI chrome (look for the Header / Sidebar description; if none exists, add it after the top-level UI overview).

- [ ] **Step 2: Add a subsection to `SPEC_TECHNIQUE.md`**

Add:
- New GraphQL query `searchableTasks: [SearchableTask!]!` with the projected field list.
- Filter: tasks where `tracking_state != Dismissed`.
- Frontend: `SearchProvider` mounts inside `BrowserRouter`; client-side Fuse.js with the documented weights (40/25/15/8/7/5) and `threshold: 0.35`, `minMatchCharLength: 2`.
- `TaskCard` reads `useSearch()` and applies `ring-2 ring-blue-500 ring-offset-2` on match, `opacity-40 grayscale-[30%]` on non-match.

- [ ] **Step 3: Commit**

```bash
git add SPEC_FONCTIONNELLE.md SPEC_TECHNIQUE.md
git commit -m "docs(spec): document global card search bar"
```

---

## Task 16: Final verification

- [ ] **Step 1: Full backend tests**

Run: `cd backend && cargo test`
Expected: all green.

- [ ] **Step 2: Full backend lint**

Run: `cd backend && cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Full frontend tests**

Run: `cd frontend && pnpm test`
Expected: all green.

- [ ] **Step 4: Frontend build + lint**

Run: `cd frontend && pnpm build && pnpm lint`
Expected: clean build, no lint errors.

- [ ] **Step 5: E2E**

Ensure backend is running, then:
Run: `cd frontend && pnpm test:e2e`
Expected: all green.

- [ ] **Step 6: Manual browser smoke**

Open `http://localhost:3000/dashboard`:
- `/` focuses the bar; type — dropdown appears, matching cards ring-highlighted, non-matching dimmed.
- `Esc` clears — everything returns to normal.
- `Cmd/Ctrl+K` focuses the bar.
- Pick a suggestion from the dropdown → edit sheet opens.
- Navigate to Priority Matrix while a query is active → matching cards on the new screen are highlighted without re-typing.

- [ ] **Step 7: Push the branch and open a PR (if desired)**

```bash
git push -u origin search
# then `gh pr create ...` per your normal workflow
```
