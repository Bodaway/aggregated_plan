# Card Search Highlight Bar — Design Spec

**Date:** 2026-04-21
**Feature:** Global JIRA-style task search bar in the app Header that highlights matching `TaskCard`s on the current screen, shows a suggestion dropdown with top matches across all tasks, and opens the existing `TaskEditSheet` when a suggestion is picked.

---

## Problem

The cockpit spreads tasks across many screens (Dashboard, Triage, Priority, Workload, Activity). When the user remembers a task by title or Jira key, there is currently no fast way to locate it. Finding a specific card means scrolling the right screen and scanning titles. Adding a search bar is the obvious fix, but the priority matrix and workload layouts carry spatial meaning — filtering cards out would destroy that information. A highlight-on-match behavior preserves layout while directing the eye to results.

---

## Goals

- A single search input, always visible in the Header, that focuses with `/` or `Cmd/Ctrl+K`.
- Typing (≥ 2 chars) highlights matching `TaskCard`s visible on the current screen (blue ring) and dims non-matching cards to 40%.
- A suggestion dropdown under the input lists up to 8 top matches across the entire task set, regardless of which screen the user is on.
- Clicking a suggestion opens the existing `TaskEditSheet` for that task without navigating away.
- Fuzzy matching (typo-tolerant) across title, Jira key, tags, project, assignee, description — weighted in that order.

## Non-goals

- Searching meetings, activity slots, alerts, dedup suggestions, or projects. (Tasks only for this iteration.)
- Server-side / paginated search. Client-side Fuse.js over the full task set is sufficient at cockpit scale.
- Route-driven, shareable search URLs. The query is session state, not URL state.
- A new read-only task detail view. Click-through reuses `TaskEditSheet`.
- Cross-screen navigation on pick. Picking a suggestion only opens the sheet.

---

## Architecture

A single `SearchProvider` at the app root owns all search state and the index. It is placed inside the urql provider and wraps the router so every route has access.

```
App
└── UrqlProvider
    └── SearchProvider                    (NEW)
        ├── Router
        │   └── PageLayout
        │       ├── Sidebar
        │       ├── Header
        │       │   └── HeaderSearchBar   (NEW)
        │       │       ├── <input>
        │       │       └── SuggestionDropdown (NEW)
        │       └── main > route screens
        │           └── TaskCard          (CHANGED)
        └── TaskEditSheet                 (EXISTING, lifted here)
```

### State owned by `SearchProvider`

| Field | Type | Notes |
|---|---|---|
| `query` | `string` | Controlled input value. |
| `allTasks` | `SearchableTask[]` | Loaded once via `searchableTasks` GraphQL query; refetched on window focus and after task mutations. |
| `fuse` | `Fuse<SearchableTask>` | Memoized on `allTasks`. |
| `matches` | `FuseResult<SearchableTask>[]` | `fuse.search(query).slice(0, 50)`; `[]` when `!highlightActive`. |
| `matchedIds` | `ReadonlySet<string>` | Derived from `matches`. |
| `highlightActive` | `boolean` | `query.trim().length >= 2 && allTasks loaded`. |
| `openTaskId` | `string \| null` | Drives the single `TaskEditSheet` instance. |

### Context API

```ts
interface SearchContextValue {
  query: string;
  setQuery: (q: string) => void;
  matches: FuseResult<SearchableTask>[];
  matchedIds: ReadonlySet<string>;
  highlightActive: boolean;
  openTaskId: string | null;
  openTaskInSheet: (id: string) => void;
  closeSheet: () => void;
  clearQuery: () => void;
}
```

### Data flow

1. On mount, `SearchProvider` fires `searchableTasks`. When data lands, the Fuse index is built.
2. User types → `setQuery` → `matches` recomputed synchronously (Fuse is fast for hundreds of items).
3. `HeaderSearchBar` renders the suggestion dropdown from `matches`. Every rendered `TaskCard` observes `matchedIds` via context and applies ring or dim classes.
4. Pick a suggestion → `openTaskInSheet(id)` sets `openTaskId` → `TaskEditSheet` opens. The user stays on the current screen.
5. `Esc` or the "×" button in the bar calls `clearQuery` → highlights disappear, dropdown closes.

---

## Backend

### New GraphQL query

```graphql
type SearchableTask {
  id: ID!
  title: String!
  sourceId: String      # Jira key, e.g. "PROJ-123"
  source: TaskSource!   # JIRA | EXCEL | PERSONAL
  assignee: String
  projectName: String
  tags: [String!]!
  description: String
  status: TaskStatus!
}

type Query {
  searchableTasks: [SearchableTask!]!
}
```

Projection-only: excludes hours, deadlines, urgency, impact — fields not needed to match or render a suggestion row. Keeps the payload lean.

### Resolver

Add `searchable_tasks` to `crates/api/src/graphql/query.rs`. The resolver delegates to a new repository method:

```rust
// crates/application/src/repositories/task_repository.rs
#[async_trait]
pub trait TaskRepository: Send + Sync {
    // ... existing methods ...
    async fn list_searchable(&self, user_id: &UserId) -> RepositoryResult<Vec<Task>>;
}
```

`list_searchable` returns every non-dismissed task for the user — `tracking_state != Dismissed`. Done tasks are included (a recently-completed task is a common lookup target). No pagination — revisit only if a user ever exceeds ~5k tasks.

### Infrastructure

Implement `list_searchable` in `crates/infrastructure/src/database/task_repo.rs` as a straightforward `SELECT ... WHERE user_id = ? AND status != 'archived'` plus the existing task-loading joins (tags, etc.).

---

## Frontend

### Package additions

- `fuse.js` (~9 KB gz) in `frontend/package.json`.

### Fuse config

`frontend/src/lib/search/fuse-config.ts`:

```ts
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
  includeMatches: true,   // for bolding matched chars in suggestion titles
  minMatchCharLength: 2,
};
```

### `SearchProvider`

`frontend/src/lib/search/SearchProvider.tsx`. Wraps children, provides `SearchContext`, owns the single `TaskEditSheet` instance, exposes `useSearch()` hook. Refetches `searchableTasks`:
- on mount,
- on `window.focus`,
- when `openTaskInSheet → onUpdated` fires (the existing `TaskEditSheet` callback).

### `HeaderSearchBar`

`frontend/src/components/search/HeaderSearchBar.tsx`. Lives inside `Header.tsx`, right-aligned, fixed width `w-80`. Controlled input bound to `query`. Renders `<SuggestionDropdown>` when `highlightActive && isFocused`.

Global keyboard listener (attached in `useEffect` on the `window`):
- `/` → focus input. **Ignored** when `document.activeElement` is an `<input>`, `<textarea>`, or `contentEditable` element.
- `Cmd+K` / `Ctrl+K` → focus input; `preventDefault()` to suppress browser defaults.
- `Esc` while input is focused → `clearQuery()`, blur input.
- `ArrowDown` / `ArrowUp` in focused input → move active suggestion in dropdown.
- `Enter` → open the active suggestion.
- `Tab` → leave dropdown without picking.

Accessibility: `role="combobox"`, `aria-expanded`, `aria-controls`, `aria-activedescendant`. Dropdown uses `role="listbox"`; rows use `role="option"`.

### `SuggestionDropdown`

`frontend/src/components/search/SuggestionDropdown.tsx`. Absolutely positioned under the input, `max-h-96 overflow-y-auto`, shadow, rounded. Up to 8 visible rows; more accessible by scroll.

Row layout (two lines):

```
[source icon]  {title with matched chars in <strong>}
               {sourceId} · {projectName} · {assignee}
```

Matched chars come from `result.matches` (Fuse `includeMatches: true`); we render the title by splitting on match indices and wrapping matched ranges in `<strong>`.

Empty state when `matches.length === 0`: a single non-interactive row reading `No tasks match "{query}"`.

Row click: `openTaskInSheet(id)` then `clearQuery()` (closes the dropdown; sheet stays open).

### `TaskCard` change

Single change in `frontend/src/components/task/TaskCard.tsx`. Consume the context and add highlight classes to the root element:

```tsx
const { highlightActive, matchedIds } = useSearch();
const isMatch = matchedIds.has(task.id);
const highlightClasses = !highlightActive
  ? ''
  : isMatch
    ? 'ring-2 ring-blue-500 ring-offset-2'
    : 'opacity-40 grayscale-[30%]';

// applied via clsx() alongside existing classes
```

No structural changes, no prop additions. The transition from normal → highlighted and back is instant (no animation); motion is reserved for user-initiated actions elsewhere.

### `TaskEditSheet` consolidation

Today `<TaskEditSheet>` is instantiated in three pages — `DashboardPage.tsx`, `PriorityMatrixPage.tsx`, and `TriagePage.tsx` — each owning its own `taskId` state. Consolidate: render exactly one `<TaskEditSheet taskId={openTaskId} onClose={closeSheet} onUpdated={refetchSearchable} />` inside `SearchProvider`. Remove the per-page instances and local state; the page-level card click handlers now call `openTaskInSheet(task.id)` from the context. The `useTaskEdit` hook inside `TaskEditSheet` is unchanged — it only fetches/updates by id.

---

## Behavior details

- **Query persistence across navigation.** The query and matches persist when the user navigates via the Sidebar, so they can type on Dashboard, jump to Priority Matrix, and still see the ring on the matched card. Cleared by `Esc` or the "×" button only.
- **Pick from dropdown.** Opens the sheet, clears the query (so the dropdown closes and highlights drop). The sheet stays open.
- **Pick a task that is visible on the current screen.** The ring is visible before the click; after the click the query clears, the sheet opens, and the ring goes away. This matches "I found it → I'm acting on it" intent.
- **Pick a task that is not on the current screen.** Sheet opens, current screen is unchanged. No auto-navigation in this iteration.
- **Sheet open while the user keeps typing.** Supported: the sheet is a separate concern from the query. Closing the sheet does not affect the query.

---

## Edge cases

- **`searchableTasks` query fails.** Bar renders a disabled input with placeholder `"Search unavailable — retry"` and a retry affordance. Highlights are simply disabled (`highlightActive` never becomes `true`).
- **Data still loading.** Input accepts typing, but the dropdown shows a single skeleton row reading `"Indexing tasks…"`. Highlights disabled until data lands; then they apply immediately.
- **Query shorter than 2 chars.** Dropdown hidden, no highlights.
- **Matched task not on the current screen.** No visible highlight on the screen; user still sees it in the dropdown.
- **Task deleted while its sheet is open.** `TaskEditSheet` already closes itself on missing-task responses — behavior unchanged.
- **Very long descriptions.** Contribute to matching but never rendered in the suggestion dropdown (only title and the meta line are shown).
- **User types while another input has focus.** `/` is ignored; `Cmd/Ctrl+K` is not.

---

## Testing

### Unit / component (Vitest + RTL) in `frontend/src/lib/search/*.test.ts(x)` and `frontend/src/components/search/*.test.tsx`

1. **`SearchProvider` state transitions** — setting query updates `matches`; clearing resets `matchedIds`; `highlightActive` flips at length 2.
2. **Fuse ranking** — given a 10-task fixture, typing `"PROJ-1"` ranks the `PROJ-12` task above a `"project planning"` task (sourceId weight > projectName weight).
3. **`HeaderSearchBar` shortcuts** — `/` focuses; `Cmd+K` focuses; `Esc` clears; `/` ignored when a `<textarea>` is focused.
4. **`SuggestionDropdown` interactions** — arrow keys move active; `Enter` calls `openTaskInSheet` with the correct id; click has identical behavior.
5. **`TaskCard` highlight classes** — plain when `highlightActive=false`; ring when in `matchedIds`; dim when active and not matched.

### Integration (RTL + in-memory urql)

Render `<App>` with a mocked `searchableTasks` response covering a small fixture. Type into the bar; assert the right cards get `ring-*` classes and non-matched cards get `opacity-*` classes.

### E2E (Playwright) — one happy path

Open Dashboard, press `/`, type part of a task title, see the card highlight, click the suggestion, see the edit sheet open with the right task id.

### Backend (Rust)

- `TaskRepository::list_searchable` returns all non-dismissed tasks for a given user; excludes tasks with `tracking_state = Dismissed`; includes `Done`.
- `searchable_tasks` GraphQL resolver returns the correct `SearchableTask` projection.

---

## Spec maintenance

Per `CLAUDE.md`, update `SPEC_FONCTIONNELLE.md` and `SPEC_TECHNIQUE.md` when this ships: add a "Recherche globale" section describing the Header search bar, the `/` and `Cmd/Ctrl+K` shortcuts, the highlight behavior, and the new `searchableTasks` GraphQL query.
