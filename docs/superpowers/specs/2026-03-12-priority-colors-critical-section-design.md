# Priority Colors & Critical Section Design

**Date:** 2026-03-12
**Feature:** Urgency-based color coding on task cards + Critical section in Priority Matrix + urgency-based sort in Dashboard.

---

## Problem

Tasks at different urgency levels look identical. Critical urgency tasks are not visually distinguished from Low urgency tasks in either the Dashboard or the Priority Matrix. The Dashboard sorts by Eisenhower quadrant but ignores the urgency level within each quadrant.

---

## Goals

- Visual urgency indicator (left-border color strip) on every `TaskCard` in compact and full mode.
- A dedicated **Critical** section above the Eisenhower 2×2 grid in the Priority Matrix; Critical tasks do not appear in the grid below.
- Dashboard day columns sorted by urgency descending (Critical first), then by impact descending.

---

## No Backend Changes

All changes are frontend-only. Urgency is already returned as a number (1–4) in all existing GraphQL queries. No new levels, no migrations.

---

## Color Scale

| Urgency value | Level    | Left border color    | Tailwind class       |
|---------------|----------|----------------------|----------------------|
| 4             | Critical | `#dc2626` (red-600)  | `border-l-red-600`   |
| 3             | High     | `#ea580c` (orange-600)| `border-l-orange-600`|
| 2             | Medium   | `#ca8a04` (yellow-600)| `border-l-yellow-600`|
| 1             | Low      | `#9ca3af` (gray-400) | `border-l-gray-400`  |

---

## Component Changes

### `TaskCard` (`frontend/src/components/task/TaskCard.tsx`)

Add a `urgencyBorderClass` helper derived from the `urgency: number` prop:

```ts
function urgencyBorderClass(urgency: number): string {
  if (urgency >= 4) return 'border-l-red-600';
  if (urgency === 3) return 'border-l-orange-600';
  if (urgency === 2) return 'border-l-yellow-600';
  return 'border-l-gray-400';
}
```

Apply to the outermost `div` in **both compact and full modes** by adding `border-l-4 ${urgencyBorderClass(urgency)}` to the existing class string.

- Compact outermost div currently: `bg-white rounded-md border border-gray-200 p-2.5 hover:shadow-sm transition-shadow`
- New: `bg-white rounded-md border border-gray-200 border-l-4 ${urgencyBorderClass(urgency)} p-2.5 hover:shadow-sm transition-shadow`

Same pattern for full mode outermost div.

---

### `PriorityMatrixPage` (`frontend/src/pages/PriorityMatrixPage.tsx`)

Add a **Critical section** above `<PriorityGrid>`.

**Logic:**
1. Collect critical tasks: union of `data.urgentImportant` and `data.urgent` where `task.urgency >= 4`.
2. Build a filtered data object for the grid: remove those tasks from their respective quadrant arrays.
3. Render Critical section only when there is at least one critical task.

**Critical section structure:**
```tsx
{criticalTasks.length > 0 && (
  <div className="mb-4 rounded-lg border border-red-200 bg-red-50 overflow-hidden">
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
          <TaskCard ... compact onClick={() => setEditingTaskId(task.id)} />
        </div>
      ))}
    </div>
  </div>
)}
<PriorityGrid data={filteredData} ... />
```

`filteredData` has the same shape as `data` but with critical tasks removed from `urgentImportant` and `urgent`.

---

### `DashboardPage` (`frontend/src/pages/DashboardPage.tsx`)

Replace the current sort (quadrant-only) with urgency-primary, impact-secondary:

```ts
// Old
const sortedTasks = [...tasks].sort(
  (a, b) => getQuadrantPriority(a.quadrant) - getQuadrantPriority(b.quadrant),
);

// New
const sortedTasks = [...tasks].sort((a, b) => {
  if (b.urgency !== a.urgency) return b.urgency - a.urgency;  // urgency desc
  return b.impact - a.impact;                                  // impact desc
});
```

This replaces `getQuadrantPriority` entirely in the DayColumn sort. The `QUADRANT_PRIORITY` map and `getQuadrantPriority` function can be removed.

---

## Files Affected

| File | Change |
|------|--------|
| `frontend/src/components/task/TaskCard.tsx` | Add `urgencyBorderClass` helper; apply `border-l-4` + color class to outermost div in both modes |
| `frontend/src/pages/PriorityMatrixPage.tsx` | Add Critical section above PriorityGrid; filter critical tasks from grid data |
| `frontend/src/pages/DashboardPage.tsx` | Replace quadrant sort with urgency desc → impact desc sort |

---

## Out of Scope

- New urgency level (no Emergency/level 5)
- Backend changes
- Color on tags, badges beyond the left border
- Drag-to-change-urgency
