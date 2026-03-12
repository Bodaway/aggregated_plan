# Priority Colors & Critical Section Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add urgency-based left-border color to every task card, surface Critical tasks in a dedicated section above the Priority Matrix grid, and sort Dashboard day columns by urgency descending.

**Architecture:** Three independent frontend-only changes: (1) a color helper in `TaskCard` applied to compact and full modes, (2) computed `criticalTasks` and `filteredData` variables in `PriorityMatrixPage` with a new Critical strip rendered above `PriorityGrid`, (3) replacement of the quadrant-based sort in `DashboardPage` with urgency→impact descending. No backend changes.

**Tech Stack:** React 18, TypeScript (strict), Tailwind CSS 3, pnpm

---

## Chunk 1: TaskCard Urgency Colors

### Task 1: Add urgency left-border color to `TaskCard`

**Files:**
- Modify: `frontend/src/components/task/TaskCard.tsx`

**Background:** `TaskCard` receives `urgency: number` (1=Low, 2=Medium, 3=High, 4=Critical) in its props interface but never uses it. The component has two render paths: `compact` (lines ~115–148) and full (lines ~152–246). Both outermost divs need a `border-l-4` + color class based on urgency.

- [ ] **Step 1: Add `urgencyBorderClass` helper before the component**

Find the line just before `export function TaskCard(` and insert:

```ts
function urgencyBorderClass(urgency: number): string {
  if (urgency >= 4) return 'border-l-red-600';   // Critical
  if (urgency === 3) return 'border-l-orange-600'; // High
  if (urgency === 2) return 'border-l-yellow-600'; // Medium
  return 'border-l-gray-400';                      // Low (1) and fallback
}
```

- [ ] **Step 2: Add `urgency` to the function's destructuring**

Find the current function signature (around line 95):
```ts
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
```

Add `urgency,` after `jiraStatus,`:
```ts
export function TaskCard({
  title,
  source,
  sourceId,
  status,
  jiraStatus,
  urgency,
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
```

- [ ] **Step 3: Apply border class to compact mode outermost div**

Find the compact outermost div (around line 118):
```tsx
className={`bg-white rounded-md border border-gray-200 p-2.5 hover:shadow-sm transition-shadow ${onClick ? 'cursor-pointer' : ''}`}
```

Replace with:
```tsx
className={`bg-white rounded-md border border-gray-200 border-l-4 ${urgencyBorderClass(urgency)} p-2.5 hover:shadow-sm transition-shadow ${onClick ? 'cursor-pointer' : ''}`}
```

- [ ] **Step 4: Apply border class to full mode outermost div**

Find the full mode outermost div (around line 157):
```tsx
className={`bg-white rounded-lg border border-gray-200 p-4 hover:shadow-sm transition-shadow ${onClick ? 'cursor-pointer' : ''}`}
```

Replace with:
```tsx
className={`bg-white rounded-lg border border-gray-200 border-l-4 ${urgencyBorderClass(urgency)} p-4 hover:shadow-sm transition-shadow ${onClick ? 'cursor-pointer' : ''}`}
```

- [ ] **Step 5: Verify TypeScript compiles**

```bash
cd /home/mbt/appfactory/aggregated_plan/frontend && pnpm tsc --noEmit
```
Expected: no errors

- [ ] **Step 6: Commit**

```bash
cd /home/mbt/appfactory/aggregated_plan && git add frontend/src/components/task/TaskCard.tsx
git commit -m "feat(frontend): add urgency-based left-border color to TaskCard"
```

---

## Chunk 2: Priority Matrix Critical Section

### Task 2: Add Critical section to `PriorityMatrixPage`

**Files:**
- Modify: `frontend/src/pages/PriorityMatrixPage.tsx`

**Background:** The page currently renders a header, `<PriorityGrid data={data} ...>`, and `<TaskEditSheet>`. `data` is `PriorityMatrixData | null` with four quadrant arrays (`urgentImportant`, `important`, `urgent`, `neither`), each holding `MatrixTask[]` where `urgency` is a number 1–4. Critical tasks (`urgency >= 4`) need to be extracted into a dedicated strip rendered **above** `PriorityGrid`. Those tasks must not appear in the grid below.

The current imports are:
```ts
import { useState, useCallback } from 'react';
import { usePriorityMatrix } from '@/hooks/use-priority-matrix';
import { PriorityGrid } from '@/components/priority/PriorityGrid';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
import type { QuadrantKey } from '@/hooks/use-priority-matrix';
```

- [ ] **Step 1: Add missing imports**

Add two lines to the import block:
```ts
import { TaskCard } from '@/components/task/TaskCard';
import type { PriorityMatrixData } from '@/hooks/use-priority-matrix';
```

The full import block becomes:
```ts
import { useState, useCallback } from 'react';
import { usePriorityMatrix } from '@/hooks/use-priority-matrix';
import { PriorityGrid } from '@/components/priority/PriorityGrid';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';
import { TaskCard } from '@/components/task/TaskCard';
import type { QuadrantKey, PriorityMatrixData } from '@/hooks/use-priority-matrix';
```

- [ ] **Step 2: Compute `criticalTasks` and `filteredData` after the `usePriorityMatrix` destructure**

Find this line:
```ts
const { data, loading, error, updatePriority } = usePriorityMatrix();
```

Add these two variables immediately after it:
```ts
const criticalTasks = data
  ? [
      ...data.urgentImportant,
      ...data.important,
      ...data.urgent,
      ...data.neither,
    ].filter(t => t.urgency >= 4)
  : [];

const filteredData: PriorityMatrixData | null = data
  ? {
      urgentImportant: data.urgentImportant.filter(t => t.urgency < 4),
      important: data.important.filter(t => t.urgency < 4),
      urgent: data.urgent.filter(t => t.urgency < 4),
      neither: data.neither.filter(t => t.urgency < 4),
    }
  : null;
```

- [ ] **Step 3: Replace `data` with `filteredData` in the `PriorityGrid` call**

Find:
```tsx
<PriorityGrid data={data} onMoveTask={handleMoveTask} onEdit={handleEdit} onDragStartExternal={() => setEditingTaskId(null)} />
```

Replace with:
```tsx
<PriorityGrid data={filteredData!} onMoveTask={handleMoveTask} onEdit={handleEdit} onDragStartExternal={() => setEditingTaskId(null)} />
```

Note: `filteredData` is typed `PriorityMatrixData | null` but is null only when `data` is null. `PriorityMatrixPage` has early returns that guarantee `data` is non-null before this JSX renders, so the non-null assertion (`!`) is safe.

- [ ] **Step 4: Add the Critical section JSX above `<PriorityGrid>`**

Find the `{/* Priority grid */}` comment and the `<PriorityGrid ...>` line. Insert the Critical section between the header block and `<PriorityGrid>`:

```tsx
      {/* Critical section */}
      {criticalTasks.length > 0 && (
        <div className="rounded-lg border border-red-200 bg-red-50 overflow-hidden mb-4">
          <div className="flex items-center gap-2 px-4 py-2 border-b border-red-200">
            <span className="text-xs font-bold tracking-widest text-red-700 uppercase">● Critical</span>
            <span className="text-xs font-semibold text-red-600 bg-red-100 rounded-full px-2 py-0.5">
              {criticalTasks.length}
            </span>
            <span className="ml-auto text-xs text-red-400">Requires immediate attention</span>
          </div>
          <div className="flex flex-wrap gap-3 p-4">
            {criticalTasks.map(task => (
              <div key={task.id} className="w-64">
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
                  onClick={() => setEditingTaskId(task.id)}
                />
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Priority grid */}
```

- [ ] **Step 5: Verify TypeScript compiles**

```bash
cd /home/mbt/appfactory/aggregated_plan/frontend && pnpm tsc --noEmit
```
Expected: no errors

- [ ] **Step 6: Commit**

```bash
cd /home/mbt/appfactory/aggregated_plan && git add frontend/src/pages/PriorityMatrixPage.tsx
git commit -m "feat(frontend): add Critical section above priority matrix grid"
```

---

## Chunk 3: Dashboard Sort

### Task 3: Replace quadrant sort with urgency-first sort in `DashboardPage`

**Files:**
- Modify: `frontend/src/pages/DashboardPage.tsx`

**Background:** `DashboardPage` currently defines `QUADRANT_PRIORITY` and `getQuadrantPriority` at module level (lines ~46–55), and uses them in `DayColumn`'s `sortedTasks` (lines ~177–179). These need to be replaced with a sort that orders tasks by `urgency` descending, then `impact` descending.

- [ ] **Step 1: Remove `QUADRANT_PRIORITY` and `getQuadrantPriority`**

Find and delete these lines (around lines 46–55):
```ts
const QUADRANT_PRIORITY: Record<string, number> = {
  UrgentImportant: 0,
  Important: 1,
  Urgent: 2,
  Neither: 3,
};

function getQuadrantPriority(q: string): number {
  return QUADRANT_PRIORITY[q] ?? 4;
}
```

- [ ] **Step 2: Update the `sortedTasks` sort inside `DayColumn`**

Find (around line 177):
```ts
  const sortedTasks = [...tasks].sort(
    (a, b) => getQuadrantPriority(a.quadrant) - getQuadrantPriority(b.quadrant),
  );
```

Replace with:
```ts
  const sortedTasks = [...tasks].sort((a, b) => {
    if (b.urgency !== a.urgency) return b.urgency - a.urgency; // urgency desc
    return b.impact - a.impact;                                 // impact desc
  });
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd /home/mbt/appfactory/aggregated_plan/frontend && pnpm tsc --noEmit
```
Expected: no errors

- [ ] **Step 4: Build to confirm no bundler issues**

```bash
cd /home/mbt/appfactory/aggregated_plan/frontend && pnpm build
```
Expected: build succeeds

- [ ] **Step 5: Commit**

```bash
cd /home/mbt/appfactory/aggregated_plan && git add frontend/src/pages/DashboardPage.tsx
git commit -m "feat(frontend): sort dashboard tasks by urgency desc, then impact desc"
```
