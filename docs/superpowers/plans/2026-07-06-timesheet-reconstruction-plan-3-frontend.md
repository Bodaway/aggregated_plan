# Timesheet Reconstruction — Plan 3: React `/timesheet` Review Screen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A `/timesheet` React screen that loads the day's reconstructed Gryzzly timesheet (via the Plan-2 GraphQL contract), shows it as a half-day timeline (locked meeting blocks + work blocks coloured by project) beside a per-project hours sidebar, and lets the user edit line hours, pin values, mark the day off, reconstruct-from-signals, and validate — the visual counterpart to the `aplan timesheet` CLI.

**Architecture:** One data hook (`use-timesheet`) owns all GraphQL (inline query/mutation strings + hand-written TS interfaces, urql `useQuery`/`useMutation` with `network-only` refetch — the project's established pattern; there is NO codegen). Three presentation pieces — `TimesheetTimeline` (adapted from `ActivityTimeline`), `ProjectSummarySidebar` (editable line rows + validate), and `TimesheetPage` (day nav + wiring) — plus a route and a nav item. Editing is line-level (numeric hours + pin), matching the dropped-`reassignBlock` decision; the timeline is display-only.

**Tech Stack:** React 18 + TypeScript (strict), Vite, urql (client at `src/lib/urql-client.ts`), Tailwind 3 (utility classes, no design system), vitest + @testing-library/react (unit), Playwright (e2e, boots the real backend). Path alias `@/*` → `src/*`.

## Global Constraints

- **Base:** branch `feat/timesheet-frontend` off `main` (which carries Plans 1, 2, 4 — the server exposes all timesheet ops). Consumes GraphQL: `timesheetDraft`, `runTimesheetReconstruction`, `saveTimesheetDraft`, `validateTimesheet`, `markDayOff`, `signalMappings`, `learnMapping`, `gryzzlyTasks` (already wired in `use-gryzzly-tasks.ts`).
- **NO graphql-codegen, NO shared generated types.** Each hook hand-writes: a template-string query/mutation + a matching `interface`. Mirror `src/hooks/use-activity.ts` / `use-gryzzly-tasks.ts`. The `src/graphql/*.graphql` files are dead — do NOT touch/rely on them.
- **NO `components/ui/` / shadcn.** Build controls with inline Tailwind on native elements, copying classes from `src/components/activity/ActivitySlotSheet.tsx` (slide-in panel: backdrop `fixed inset-0 bg-black/20 z-40`, panel `fixed top-0 right-0 h-full max-w-md`, Escape-to-close) and `ActivityTimeline.tsx`.
- **GraphQL enum values arrive SCREAMING_SNAKE** (async-graphql default): status `DRAFT|VALIDATED|SUBMITTED|DAY_OFF`, confidence `HIGH|MEDIUM|LOW`, block kind `MEETING|WORK|OUT_OF_OFFICE`, `DayOffScopeGql` = `FULL|MORNING|AFTERNOON`. `TimesheetLineInput` = `{ gryzzlyProjectId: ID (nullable), hours: Float!, isPinned: Boolean! }`. `$date` type is `NaiveDate!`; pass `formatDate(date)` (`yyyy-MM-dd`).
- **Tests: vitest** — `import { describe, it, expect, vi } from 'vitest'`; components using urql are unit-tested by mocking `vi.mock('urql', ...)` (no Provider needed). Commands: `pnpm test` (vitest run), `pnpm build` (tsc typecheck + vite build), `pnpm test:e2e` (Playwright; boots backend + dev server). Run from `frontend/`.
- **Strict TS** (all strict flags): no `any` in committed code, handle `null` explicitly (lines/blocks project ids are nullable).
- **Reconstruct never silently clobbers edits:** the screen LOADS `timesheetDraft` (persisted, preserves edits). "Refresh from signals" (`runTimesheetReconstruction`) is an explicit, confirmed action. On a day with no draft yet, auto-run reconstruction once so the screen is immediately useful.
- **Commits:** imperative subject, no Jira key, NO `Co-Authored-By`; stage only task-relevant files. TDD where practical (component tests alongside).

---

## File Structure

**Created:**
- `frontend/src/hooks/use-timesheet.ts` — GraphQL ops + interfaces + the `useTimesheet(date)` hook.
- `frontend/src/hooks/use-timesheet.test.ts` — hook query/interface smoke (urql mocked).
- `frontend/src/components/timesheet/TimesheetTimeline.tsx` — half-day timeline of blocks.
- `frontend/src/components/timesheet/TimesheetTimeline.test.tsx`
- `frontend/src/components/timesheet/ProjectSummarySidebar.tsx` — editable per-project rows + totals + actions.
- `frontend/src/components/timesheet/ProjectSummarySidebar.test.tsx`
- `frontend/src/components/timesheet/project-colors.ts` — stable projectId→color hash (shared).
- `frontend/src/pages/TimesheetPage.tsx` — day nav + wiring.
- `frontend/src/pages/TimesheetPage.test.tsx`

**Modified:**
- `frontend/src/App.tsx` — add the `/timesheet` route.
- `frontend/src/components/layout/Sidebar.tsx` — add the nav item.
- `frontend/e2e/smoke.spec.ts` — add a `can navigate to timesheet` case.
- `SPEC_FONCTIONNELLE.md` — document the screen (French).

---

### Task 1: `use-timesheet` hook (GraphQL + types)

**Files:**
- Create: `frontend/src/hooks/use-timesheet.ts`, `frontend/src/hooks/use-timesheet.test.ts`

**Interfaces:**
- Produces: TS interfaces `TimesheetLine`, `AttributedBlock`, `UnresolvedSignal`, `ReconstructedDay`, `TimesheetLineInput`; hook `useTimesheet(date: Date)` returning `{ day: ReconstructedDay | null, loading, error, reconstruct(): Promise<void>, saveLines(lines: TimesheetLineInput[]): Promise<void>, validate(): Promise<void>, markOff(scope: 'FULL'|'MORNING'|'AFTERNOON'): Promise<void>, refetch(): void }`.

- [ ] **Step 1: Write the hook**

Create `frontend/src/hooks/use-timesheet.ts`:
```ts
import { useCallback, useEffect, useRef } from 'react';
import { useMutation, useQuery } from 'urql';

import { formatDate } from '@/lib/date-utils';

export type Confidence = 'HIGH' | 'MEDIUM' | 'LOW';
export type TimesheetStatus = 'DRAFT' | 'VALIDATED' | 'SUBMITTED' | 'DAY_OFF';
export type BlockKind = 'MEETING' | 'WORK' | 'OUT_OF_OFFICE';
export type DayOffScope = 'FULL' | 'MORNING' | 'AFTERNOON';

export interface TimesheetLine {
  gryzzlyProjectId: string | null;
  projectName: string | null;
  hours: number;
  isPinned: boolean;
  confidence: Confidence;
  sourceRefs: string[];
}
export interface AttributedBlock {
  startTime: string;
  endTime: string;
  gryzzlyProjectId: string | null;
  kind: BlockKind;
  hours: number;
  sourceRefs: string[];
}
export interface UnresolvedSignal {
  sourceRef: string;
  label: string;
  at: string;
}
export interface ReconstructedDay {
  date: string;
  status: TimesheetStatus;
  targetHours: number;
  roundingIncrement: number;
  totalHours: number;
  dayConfidence: Confidence;
  lines: TimesheetLine[];
  unattributedHours: number;
  unresolved: UnresolvedSignal[];
  blocks: AttributedBlock[];
}
export interface TimesheetLineInput {
  gryzzlyProjectId: string | null;
  hours: number;
  isPinned: boolean;
}

// Shared selection set for every op that returns a ReconstructedDay.
const DAY_FIELDS = `
  date status targetHours roundingIncrement totalHours dayConfidence unattributedHours
  lines { gryzzlyProjectId projectName hours isPinned confidence sourceRefs }
  unresolved { sourceRef label at }
  blocks { startTime endTime gryzzlyProjectId kind hours sourceRefs }
`;

const TIMESHEET_DRAFT_QUERY = `query TimesheetDraft($date: NaiveDate!) { timesheetDraft(date: $date) { ${DAY_FIELDS} } }`;
const RECONSTRUCT_MUTATION = `mutation RunReconstruction($date: NaiveDate!) { runTimesheetReconstruction(date: $date) { ${DAY_FIELDS} } }`;
const SAVE_DRAFT_MUTATION = `mutation SaveDraft($date: NaiveDate!, $lines: [TimesheetLineInput!]!) { saveTimesheetDraft(date: $date, lines: $lines) { ${DAY_FIELDS} } }`;
const VALIDATE_MUTATION = `mutation Validate($date: NaiveDate!) { validateTimesheet(date: $date) { ${DAY_FIELDS} } }`;
const MARK_DAY_OFF_MUTATION = `mutation MarkDayOff($date: NaiveDate!, $scope: DayOffScopeGql!) { markDayOff(date: $date, scope: $scope) { ${DAY_FIELDS} } }`;

interface DraftData { timesheetDraft: ReconstructedDay | null; }

export function useTimesheet(date: Date) {
  const dateStr = formatDate(date);
  const [result, reexecute] = useQuery<DraftData>({
    query: TIMESHEET_DRAFT_QUERY,
    variables: { date: dateStr },
  });
  const [, execReconstruct] = useMutation(RECONSTRUCT_MUTATION);
  const [, execSave] = useMutation(SAVE_DRAFT_MUTATION);
  const [, execValidate] = useMutation(VALIDATE_MUTATION);
  const [, execMarkOff] = useMutation(MARK_DAY_OFF_MUTATION);

  const refetch = useCallback(
    () => reexecute({ requestPolicy: 'network-only' }),
    [reexecute],
  );

  const reconstruct = useCallback(async () => {
    const res = await execReconstruct({ date: dateStr });
    if (!res.error) refetch();
  }, [execReconstruct, dateStr, refetch]);

  const saveLines = useCallback(
    async (lines: TimesheetLineInput[]) => {
      const res = await execSave({ date: dateStr, lines });
      if (!res.error) refetch();
    },
    [execSave, dateStr, refetch],
  );

  const validate = useCallback(async () => {
    const res = await execValidate({ date: dateStr });
    if (!res.error) refetch();
  }, [execValidate, dateStr, refetch]);

  const markOff = useCallback(
    async (scope: DayOffScope) => {
      const res = await execMarkOff({ date: dateStr, scope });
      if (!res.error) refetch();
    },
    [execMarkOff, dateStr, refetch],
  );

  // Auto-reconstruct ONCE per date when no draft exists yet (so the screen is useful immediately).
  const autoRanFor = useRef<string | null>(null);
  useEffect(() => {
    if (
      !result.fetching &&
      !result.error &&
      result.data &&
      result.data.timesheetDraft === null &&
      autoRanFor.current !== dateStr
    ) {
      autoRanFor.current = dateStr;
      void reconstruct();
    }
  }, [result.fetching, result.error, result.data, dateStr, reconstruct]);

  return {
    day: result.data?.timesheetDraft ?? null,
    loading: result.fetching,
    error: result.error ?? null,
    reconstruct,
    saveLines,
    validate,
    markOff,
    refetch,
  };
}
```

- [ ] **Step 2: Write a vitest smoke test (urql mocked)**

Create `frontend/src/hooks/use-timesheet.test.ts`:
```ts
import { describe, it, expect, vi } from 'vitest';
import { renderHook } from '@testing-library/react';

const reexecute = vi.fn();
vi.mock('urql', () => ({
  useQuery: () => [{ fetching: false, data: { timesheetDraft: null }, error: undefined }, reexecute],
  useMutation: () => [{ fetching: false }, vi.fn().mockResolvedValue({ error: undefined })],
}));

import { useTimesheet } from './use-timesheet';

describe('useTimesheet', () => {
  it('exposes the timesheet actions and a null day when no draft', () => {
    const { result } = renderHook(() => useTimesheet(new Date('2026-06-08T00:00:00Z')));
    expect(result.current.day).toBeNull();
    expect(typeof result.current.reconstruct).toBe('function');
    expect(typeof result.current.saveLines).toBe('function');
    expect(typeof result.current.validate).toBe('function');
    expect(typeof result.current.markOff).toBe('function');
  });
});
```

- [ ] **Step 3: Run test + typecheck**

Run: `cd frontend && pnpm test -- use-timesheet && pnpm build`
Expected: test passes; `pnpm build` (tsc) reports no type errors in the new file.
> If `renderHook` isn't exported from `@testing-library/react` at this version, use `@testing-library/react`'s `renderHook` (v14+) — it is; if the version predates it, wrap in a trivial component instead. Confirm from `package.json`.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/use-timesheet.ts frontend/src/hooks/use-timesheet.test.ts
git commit -m "Add use-timesheet hook (reconstructed-day query + edit/validate/day-off mutations)"
```

---

### Task 2: `TimesheetTimeline` component

**Files:**
- Create: `frontend/src/components/timesheet/project-colors.ts`, `frontend/src/components/timesheet/TimesheetTimeline.tsx`, `frontend/src/components/timesheet/TimesheetTimeline.test.tsx`

**Interfaces:**
- Consumes: `AttributedBlock` (Task 1).
- Produces: `projectColor(projectId: string | null): string` (stable Tailwind bg class); `<TimesheetTimeline blocks={AttributedBlock[]} />`.

- [ ] **Step 1: Stable per-project color**

Create `frontend/src/components/timesheet/project-colors.ts`:
```ts
// Stable project→colour mapping (hash the id so a project keeps its colour across days).
const SLOT_COLORS = [
  'bg-blue-400', 'bg-green-400', 'bg-purple-400', 'bg-amber-400',
  'bg-rose-400', 'bg-cyan-400', 'bg-orange-400', 'bg-teal-400',
] as const;

export function projectColor(projectId: string | null): string {
  if (!projectId) return 'bg-gray-300'; // unattributed
  let hash = 0;
  for (let i = 0; i < projectId.length; i += 1) {
    hash = (hash * 31 + projectId.charCodeAt(i)) | 0;
  }
  return SLOT_COLORS[Math.abs(hash) % SLOT_COLORS.length];
}
```

- [ ] **Step 2: Write the failing test**

Create `frontend/src/components/timesheet/TimesheetTimeline.test.tsx`:
```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';

import { TimesheetTimeline } from './TimesheetTimeline';
import type { AttributedBlock } from '@/hooks/use-timesheet';

// NOTE: bare local NaiveDateTime (no `Z`/offset) — exactly the wire format the backend
// emits — so new Date(iso).getHours() is deterministic in any test-runner timezone.
const blocks: AttributedBlock[] = [
  { startTime: '2026-06-08T08:00:00', endTime: '2026-06-08T10:00:00', gryzzlyProjectId: 'p1', kind: 'WORK', hours: 2, sourceRefs: [] },
  { startTime: '2026-06-08T09:00:00', endTime: '2026-06-08T10:00:00', gryzzlyProjectId: null, kind: 'MEETING', hours: 1, sourceRefs: [] },
  { startTime: '2026-06-08T14:00:00', endTime: '2026-06-08T16:00:00', gryzzlyProjectId: 'p1', kind: 'WORK', hours: 2, sourceRefs: [] },
];

describe('TimesheetTimeline', () => {
  it('renders morning and afternoon half-day columns', () => {
    render(<TimesheetTimeline blocks={blocks} />);
    expect(screen.getByText(/morning/i)).toBeInTheDocument();
    expect(screen.getByText(/afternoon/i)).toBeInTheDocument();
  });

  it('renders a bar per block that overlaps a half-day window', () => {
    const { container } = render(<TimesheetTimeline blocks={blocks} />);
    // 3 blocks all overlap their windows → at least 3 positioned bars.
    expect(container.querySelectorAll('[data-block]').length).toBeGreaterThanOrEqual(3);
  });

  it('shows an empty-state when there are no blocks', () => {
    render(<TimesheetTimeline blocks={[]} />);
    expect(screen.getByText(/no activity/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Implement (adapt ActivityTimeline math)**

Create `frontend/src/components/timesheet/TimesheetTimeline.tsx`:
```tsx
import type { AttributedBlock } from '@/hooks/use-timesheet';
import { projectColor } from './project-colors';

const AM_START = 8 * 60;
const AM_END = 12 * 60;
const PM_START = 13 * 60;
const PM_END = 17 * 60;

function timeToMinutes(iso: string): number {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return 0;
  return d.getHours() * 60 + d.getMinutes();
}

interface WindowDef {
  label: string;
  start: number;
  end: number;
}
const WINDOWS: WindowDef[] = [
  { label: 'Morning', start: AM_START, end: AM_END },
  { label: 'Afternoon', start: PM_START, end: PM_END },
];

function blockClasses(kind: AttributedBlock['kind'], projectId: string | null): string {
  if (kind === 'MEETING') return 'bg-slate-400 bg-[repeating-linear-gradient(45deg,transparent,transparent_4px,rgba(0,0,0,0.12)_4px,rgba(0,0,0,0.12)_8px)]';
  if (kind === 'OUT_OF_OFFICE') return 'bg-gray-200';
  return projectColor(projectId);
}

function HalfDay({ win, blocks }: { win: WindowDef; blocks: AttributedBlock[] }) {
  const duration = win.end - win.start;
  const inWindow = blocks.filter((b) => timeToMinutes(b.endTime) > win.start && timeToMinutes(b.startTime) < win.end);
  return (
    <div className="flex-1">
      <div className="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-1">{win.label}</div>
      <div className="relative h-16 rounded bg-gray-50 border border-gray-200 overflow-hidden">
        {inWindow.map((b, i) => {
          const s = Math.max(timeToMinutes(b.startTime), win.start);
          const e = Math.min(timeToMinutes(b.endTime), win.end);
          const leftPct = ((s - win.start) / duration) * 100;
          const widthPct = Math.max(((e - s) / duration) * 100, 1);
          const label = b.kind === 'MEETING' ? 'meet' : (b.gryzzlyProjectId ?? '??');
          return (
            <div
              key={`${b.startTime}-${i}`}
              data-block
              title={`${label} · ${b.hours.toFixed(2)}h`}
              className={`absolute top-1 bottom-1 rounded text-[10px] text-white/90 px-1 overflow-hidden whitespace-nowrap ${blockClasses(b.kind, b.gryzzlyProjectId)}`}
              style={{ left: `${leftPct}%`, width: `${widthPct}%` }}
            >
              {widthPct > 10 ? label : ''}
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function TimesheetTimeline({ blocks }: { blocks: AttributedBlock[] }) {
  if (blocks.length === 0) {
    return <div className="text-sm text-gray-400 italic py-4">No activity reconstructed for this day.</div>;
  }
  return (
    <div className="flex gap-6">
      {WINDOWS.map((win) => (
        <HalfDay key={win.label} win={win} blocks={blocks} />
      ))}
    </div>
  );
}
```
> **Timezone (confirmed):** the backend types these as `NaiveDateTime`, and async-graphql serializes them BARE — `2026-06-08T08:00:00`, no `Z`/offset (verified against the SDL scalar + `api/src/graphql/types/timesheet.rs`). So `new Date(iso).getHours()` (browser-local, matching `ActivityTimeline`) is correct — do NOT use `getUTCHours`. Test fixtures use bare datetimes for the same reason.

- [ ] **Step 4: Run test + typecheck; commit**

Run: `cd frontend && pnpm test -- TimesheetTimeline && pnpm build`
Expected: 3 tests pass; typecheck clean.
```bash
git add frontend/src/components/timesheet/project-colors.ts frontend/src/components/timesheet/TimesheetTimeline.tsx frontend/src/components/timesheet/TimesheetTimeline.test.tsx
git commit -m "Add TimesheetTimeline (half-day blocks coloured by project)"
```

---

### Task 3: `ProjectSummarySidebar` component

**Files:**
- Create: `frontend/src/components/timesheet/ProjectSummarySidebar.tsx`, `.test.tsx`

**Interfaces:**
- Consumes: `ReconstructedDay`, `TimesheetLine`, `TimesheetLineInput`, `projectColor`.
- Produces: `<ProjectSummarySidebar day={ReconstructedDay} onSaveLines={(lines)=>void} onValidate={()=>void} onMarkOff={(scope)=>void} onRefresh={()=>void} busy={boolean} />`. Local editable copy of line hours + pin; a "Save", "Validate & lock", "Refresh from signals", "Day off" control set; total vs target with over/under badge; confidence + status badges; unattributed row highlighted.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/components/timesheet/ProjectSummarySidebar.test.tsx`:
```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { ProjectSummarySidebar } from './ProjectSummarySidebar';
import type { ReconstructedDay } from '@/hooks/use-timesheet';

const day: ReconstructedDay = {
  date: '2026-06-08', status: 'DRAFT', targetHours: 7.5, roundingIncrement: 0.25,
  totalHours: 7.5, dayConfidence: 'HIGH', unattributedHours: 1.5, unresolved: [], blocks: [],
  lines: [
    { gryzzlyProjectId: 'p1', projectName: 'Proj One', hours: 6, isPinned: false, confidence: 'HIGH', sourceRefs: [] },
    { gryzzlyProjectId: null, projectName: null, hours: 1.5, isPinned: false, confidence: 'LOW', sourceRefs: [] },
  ],
};

describe('ProjectSummarySidebar', () => {
  it('renders each line, the unattributed row, and the total vs target', () => {
    render(<ProjectSummarySidebar day={day} onSaveLines={vi.fn()} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    expect(screen.getByText('Proj One')).toBeInTheDocument();
    expect(screen.getByText(/unattributed/i)).toBeInTheDocument();
    expect(screen.getByText(/7\.5.*\/.*7\.5/)).toBeInTheDocument(); // total / target
  });

  it('validates via the callback', () => {
    const onValidate = vi.fn();
    render(<ProjectSummarySidebar day={day} onSaveLines={vi.fn()} onValidate={onValidate} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    fireEvent.click(screen.getByRole('button', { name: /validate/i }));
    expect(onValidate).toHaveBeenCalledOnce();
  });

  it('saves edited hours (pinning the edited line) via onSaveLines', () => {
    const onSaveLines = vi.fn();
    render(<ProjectSummarySidebar day={day} onSaveLines={onSaveLines} onValidate={vi.fn()} onMarkOff={vi.fn()} onRefresh={vi.fn()} busy={false} />);
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[0], { target: { value: '5' } });
    fireEvent.click(screen.getByRole('button', { name: /save/i }));
    expect(onSaveLines).toHaveBeenCalledWith(
      expect.arrayContaining([
        expect.objectContaining({ gryzzlyProjectId: 'p1', hours: 5, isPinned: true }),
      ]),
    );
  });
});
```

- [ ] **Step 2: Implement**

Create `frontend/src/components/timesheet/ProjectSummarySidebar.tsx`:
```tsx
import { useEffect, useState } from 'react';

import type { DayOffScope, ReconstructedDay, TimesheetLineInput } from '@/hooks/use-timesheet';
import { projectColor } from './project-colors';

interface Props {
  day: ReconstructedDay;
  onSaveLines: (lines: TimesheetLineInput[]) => void;
  onValidate: () => void;
  onMarkOff: (scope: DayOffScope) => void;
  onRefresh: () => void;
  busy: boolean;
}

interface EditRow {
  gryzzlyProjectId: string | null;
  label: string;
  hours: number;
  isPinned: boolean;
  confidence: string;
}

export function ProjectSummarySidebar({ day, onSaveLines, onValidate, onMarkOff, onRefresh, busy }: Props) {
  const [rows, setRows] = useState<EditRow[]>([]);
  // Track which rows the user edited → those get pinned on save.
  const [edited, setEdited] = useState<Set<number>>(new Set());

  useEffect(() => {
    setRows(
      day.lines.map((l) => ({
        gryzzlyProjectId: l.gryzzlyProjectId,
        label: l.projectName ?? l.gryzzlyProjectId ?? 'Unattributed',
        hours: l.hours,
        isPinned: l.isPinned,
        confidence: l.confidence,
      })),
    );
    setEdited(new Set());
  }, [day]);

  const total = rows.reduce((s, r) => s + (Number.isFinite(r.hours) ? r.hours : 0), 0);
  const delta = total - day.targetHours;
  const balanced = Math.abs(delta) < 1e-6;

  const setHours = (i: number, value: number) => {
    setRows((prev) => prev.map((r, idx) => (idx === i ? { ...r, hours: value } : r)));
    setEdited((prev) => new Set(prev).add(i));
  };

  const save = () => {
    const lines: TimesheetLineInput[] = rows.map((r, i) => ({
      gryzzlyProjectId: r.gryzzlyProjectId,
      hours: r.hours,
      isPinned: r.isPinned || edited.has(i),
    }));
    onSaveLines(lines);
  };

  const locked = day.status === 'VALIDATED' || day.status === 'SUBMITTED';

  return (
    <div className="w-80 shrink-0 bg-white rounded-lg border border-gray-200 p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">Hours × project</h2>
        <span className={`text-[10px] px-2 py-0.5 rounded-full ${locked ? 'bg-green-100 text-green-700' : 'bg-gray-100 text-gray-600'}`}>
          {day.status}
        </span>
      </div>

      <div className="space-y-2">
        {rows.map((r, i) => (
          <div key={r.gryzzlyProjectId ?? `unattributed-${i}`} className="flex items-center gap-2">
            <span className={`inline-block w-3 h-3 rounded-sm ${projectColor(r.gryzzlyProjectId)}`} />
            <span className={`flex-1 text-sm truncate ${r.gryzzlyProjectId ? 'text-gray-800' : 'text-amber-700 font-medium'}`}>
              {r.gryzzlyProjectId ? r.label : 'Unattributed'}
            </span>
            {r.confidence === 'LOW' && <span title="low confidence" className="text-amber-500 text-xs">▲</span>}
            <input
              type="number"
              step={day.roundingIncrement}
              min={0}
              value={r.hours}
              disabled={locked || busy}
              onChange={(e) => setHours(i, Math.max(0, parseFloat(e.target.value) || 0))}
              className="w-16 text-right text-sm border border-gray-300 rounded px-1 py-0.5 disabled:bg-gray-100"
            />
            <span className="text-xs text-gray-400">h</span>
          </div>
        ))}
      </div>

      <div className="flex items-center justify-between border-t border-gray-100 pt-2 text-sm">
        <span className="font-medium text-gray-700">
          {total.toFixed(2)} / {day.targetHours.toFixed(1)}h
        </span>
        <span className={balanced ? 'text-green-600' : 'text-amber-600'}>
          {balanced ? '✓ balanced' : `${delta > 0 ? '+' : ''}${delta.toFixed(2)}h`}
        </span>
      </div>

      {!locked && (
        <div className="grid grid-cols-2 gap-2 pt-1">
          <button onClick={save} disabled={busy} className="bg-gray-100 text-gray-800 text-sm rounded px-2 py-1 hover:bg-gray-200 disabled:opacity-50">Save</button>
          <button onClick={onValidate} disabled={busy} className="bg-blue-600 text-white text-sm rounded px-2 py-1 hover:bg-blue-700 disabled:opacity-50">Validate &amp; lock</button>
          <button onClick={onRefresh} disabled={busy} className="bg-white border border-gray-300 text-gray-700 text-sm rounded px-2 py-1 hover:bg-gray-50 disabled:opacity-50">Refresh from signals</button>
          <button onClick={() => onMarkOff('FULL')} disabled={busy} className="bg-white border border-gray-300 text-gray-700 text-sm rounded px-2 py-1 hover:bg-gray-50 disabled:opacity-50">Day off</button>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Run test + typecheck; commit**

Run: `cd frontend && pnpm test -- ProjectSummarySidebar && pnpm build`
Expected: 3 tests pass; typecheck clean.
```bash
git add frontend/src/components/timesheet/ProjectSummarySidebar.tsx frontend/src/components/timesheet/ProjectSummarySidebar.test.tsx
git commit -m "Add ProjectSummarySidebar (editable per-project hours + validate/refresh/day-off)"
```

---

### Task 4: `TimesheetPage`

**Files:**
- Create: `frontend/src/pages/TimesheetPage.tsx`, `frontend/src/pages/TimesheetPage.test.tsx`

**Interfaces:**
- Consumes: `useTimesheet`, `TimesheetTimeline`, `ProjectSummarySidebar`, date-utils.
- Produces: default-exported `TimesheetPage` (day nav + wiring; "Refresh from signals" confirms before clobbering edits).

- [ ] **Step 1: Write the failing test (urql + child hook mocked)**

Create `frontend/src/pages/TimesheetPage.test.tsx`:
```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

const day = {
  date: '2026-06-08', status: 'DRAFT', targetHours: 7.5, roundingIncrement: 0.25,
  totalHours: 7.5, dayConfidence: 'HIGH', unattributedHours: 0, unresolved: [], blocks: [],
  lines: [{ gryzzlyProjectId: 'p1', projectName: 'Proj One', hours: 7.5, isPinned: false, confidence: 'HIGH', sourceRefs: [] }],
};
vi.mock('@/hooks/use-timesheet', () => ({
  useTimesheet: () => ({
    day, loading: false, error: null,
    reconstruct: vi.fn(), saveLines: vi.fn(), validate: vi.fn(), markOff: vi.fn(), refetch: vi.fn(),
  }),
}));

import { TimesheetPage } from './TimesheetPage';

describe('TimesheetPage', () => {
  it('renders the day summary and timeline heading', () => {
    render(<TimesheetPage />);
    expect(screen.getByText('Proj One')).toBeInTheDocument();
    expect(screen.getByText(/hours × project/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Implement**

Create `frontend/src/pages/TimesheetPage.tsx`:
```tsx
import { useState } from 'react';

import { ProjectSummarySidebar } from '@/components/timesheet/ProjectSummarySidebar';
import { TimesheetTimeline } from '@/components/timesheet/TimesheetTimeline';
import { formatDisplayDate, getNextDay, getPrevDay } from '@/lib/date-utils';
import { useTimesheet } from '@/hooks/use-timesheet';

export function TimesheetPage() {
  const [date, setDate] = useState<Date>(new Date());
  const { day, loading, error, reconstruct, saveLines, validate, markOff, refetch } = useTimesheet(date);

  const onRefresh = () => {
    if (window.confirm('Reconstruct from signals? This overwrites unsaved manual edits for this day.')) {
      void reconstruct();
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <button onClick={() => setDate((d) => getPrevDay(d))} className="px-2 py-1 text-sm rounded border border-gray-300 hover:bg-gray-50">←</button>
        <button onClick={() => setDate(new Date())} className="px-2 py-1 text-sm rounded border border-gray-300 hover:bg-gray-50">Today</button>
        <button onClick={() => setDate((d) => getNextDay(d))} className="px-2 py-1 text-sm rounded border border-gray-300 hover:bg-gray-50">→</button>
        <span className="ml-2 text-sm font-medium text-gray-700">{formatDisplayDate(date)}</span>
        <button onClick={refetch} className="ml-auto px-2 py-1 text-xs text-gray-500 hover:text-gray-800">⟳</button>
      </div>

      {error && <div className="text-sm text-red-600">Failed to load timesheet: {error.message}</div>}
      {loading && !day && <div className="text-sm text-gray-400">Reconstructing…</div>}

      {day && (
        <div className="flex gap-6 items-start">
          <div className="flex-1 bg-white rounded-lg border border-gray-200 p-4">
            <TimesheetTimeline blocks={day.blocks} />
            {day.unresolved.length > 0 && (
              <div className="mt-3 text-xs text-amber-700">
                {day.unresolved.length} unresolved signal(s) — assign hours to a project in the sidebar.
              </div>
            )}
          </div>
          <ProjectSummarySidebar
            day={day}
            onSaveLines={saveLines}
            onValidate={validate}
            onMarkOff={markOff}
            onRefresh={onRefresh}
            busy={loading}
          />
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Run test + typecheck; commit**

Run: `cd frontend && pnpm test -- TimesheetPage && pnpm build`
Expected: test passes; typecheck clean.
```bash
git add frontend/src/pages/TimesheetPage.tsx frontend/src/pages/TimesheetPage.test.tsx
git commit -m "Add TimesheetPage (day nav + timeline + project sidebar wiring)"
```

---

### Task 5: Route + nav item

**Files:**
- Modify: `frontend/src/App.tsx`, `frontend/src/components/layout/Sidebar.tsx`

- [ ] **Step 1: Add the route**

In `frontend/src/App.tsx`, import `TimesheetPage` as a NAMED import — `import { TimesheetPage } from '@/pages/TimesheetPage';` — to match the file's convention (all pages are named imports; confirm alias vs relative against the existing lines). Add, alongside the other `<Route>`s, wrapped in `<PageLayout title="Timesheet">` exactly like the neighbouring routes:
```tsx
<Route path="/timesheet" element={<PageLayout title="Timesheet"><TimesheetPage /></PageLayout>} />
```

- [ ] **Step 2: Add the nav item**

In `frontend/src/components/layout/Sidebar.tsx`, add to the `navItems` array (after the Activity item):
```tsx
{ path: '/timesheet', label: 'Timesheet', iconPath: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4' },
```
> Match the exact shape of the existing `navItems` entries (field names may be `icon`/`to`/`name` rather than `iconPath`/`path`/`label` — copy a sibling entry's shape verbatim and only change the values).

- [ ] **Step 3: Typecheck + build; commit**

Run: `cd frontend && pnpm build`
Expected: typecheck + build clean; `/timesheet` reachable.
```bash
git add frontend/src/App.tsx frontend/src/components/layout/Sidebar.tsx
git commit -m "Wire /timesheet route + sidebar nav item"
```

---

### Task 6: E2e smoke + full-suite green

**Files:**
- Modify: `frontend/e2e/smoke.spec.ts`

- [ ] **Step 1: Add the nav e2e case**

In `frontend/e2e/smoke.spec.ts`, add (mirroring the existing activity nav case):
```ts
test('can navigate to timesheet', async ({ page }) => {
  await page.goto('/dashboard');
  await page.getByRole('link', { name: /timesheet/i }).click();
  await expect(page).toHaveURL(/timesheet/);
  await expect(page.getByText(/hours × project|reconstructing/i)).toBeVisible();
});
```

- [ ] **Step 2: Run the unit suite + build**

Run: `cd frontend && pnpm test && pnpm build`
Expected: ALL vitest tests pass (existing + the 4 new files); build clean.

- [ ] **Step 3: (Best-effort) e2e**

Run: `cd frontend && pnpm test:e2e -- smoke` (boots the real backend on :3001 + dev server on :3000).
Expected: the timesheet nav case passes. If the backend can't boot in this environment (e.g. no cargo/db), record that the e2e was not run and rely on the unit + build gates — do NOT delete the e2e case.

- [ ] **Step 4: Commit**

```bash
git add frontend/e2e/smoke.spec.ts
git commit -m "Add e2e smoke case for /timesheet navigation"
```

---

### Task 7: Spec update (French)

**Files:**
- Modify: `SPEC_FONCTIONNELLE.md`

- [ ] **Step 1: Document the screen**

In `SPEC_FONCTIONNELLE.md`, add a user story / section (French) for the écran `/timesheet` : revue visuelle du jour (timeline demi-journée avec réunions verrouillées + blocs de travail colorés par projet ; sidebar heures × projet éditable), actions : éditer les heures (épingle), valider & verrouiller, reconstruire depuis les signaux (avec confirmation car écrase les éditions), marquer jour off ; c'est la contrepartie visuelle de `aplan timesheet`. Match the existing US numbering/format.

- [ ] **Step 2: Commit**

```bash
git add SPEC_FONCTIONNELLE.md
git commit -m "Document the /timesheet review screen (Surface B)"
```

---

## Self-Review

**Spec coverage (design §9.2 Surface B):**
- `/timesheet` route + nav → Task 5. ✅
- Timeline (meetings locked/hatched + work blocks coloured by project, half-day math from ActivityTimeline) → Task 2. ✅
- Per-project hours sidebar + total vs target + validate → Task 3. ✅
- Numeric-first editing (pin on edit); block-drag deferred (design said defer) → Task 3. ✅
- Consumes the Plan-2 contract via hand-written hook (no codegen) → Task 1. ✅
- Reconstruct/refresh (confirmed), mark-day-off, validate → Tasks 1,3,4. ✅
- Deferred (documented): a full signal-mapping manager UI (learnMapping/signalMappings) — the CLI `aplan map` covers rule management; the screen assigns hours via line editing. Block-level reassign (dropped in Plan 2). Follow-up.

**Placeholder scan:** The two "match the sibling's exact field shape" notes (Task 5 nav item; App.tsx import style) are concrete "copy the existing shape" instructions — the implementer confirms `navItems` field names against the real `Sidebar.tsx`. The timezone getter note (Task 2) is a verify-one-payload instruction. No TODOs.

**Type consistency:** Enum string values (`DRAFT`/`HIGH`/`MEETING`/`FULL`) used identically across hook, components, tests. `TimesheetLineInput` shape (`gryzzlyProjectId|hours|isPinned`) matches the Plan-2 GraphQL input. `AttributedBlock`/`TimesheetLine`/`ReconstructedDay` field names match the Plan-2 SDL (camelCase). `projectColor` signature identical across Timeline + Sidebar.

**Open verification notes for the implementer:**
1. `Sidebar.tsx` `navItems` real field names (`path`/`label`/`iconPath` vs `to`/`name`/`icon`) — copy a sibling verbatim.
2. `App.tsx` route wrapper (`PageLayout` prop name `title`) + import style (alias vs relative).
3. `@testing-library/react` version supports `renderHook` (Task 1) — else wrap in a component.
4. Block `startTime` serialization: **CONFIRMED bare local `NaiveDateTime` (no `Z`)** → use `getHours()` (matches `ActivityTimeline`); test fixtures use bare datetimes. Do NOT switch to `getUTCHours()`.
5. `formatDate`/`formatDisplayDate`/`getPrevDay`/`getNextDay` exist in `@/lib/date-utils` (confirmed by recon).

**Deferred follow-ups (documented, out of Plan 3 scope — need backend or later work):**
- **Validated-day recovery:** once a day is `VALIDATED`/`SUBMITTED` the sidebar actions hide (by design — locked) and there is no un-validate mutation, so a mistaken validate can only be undone via CLI/DB. Follow-up: add a backend `reopenTimesheet(date)` mutation + a "Re-open" button. (Day-nav still works; only that day is locked — not a whole-app dead-end.)
- **Half-day off:** the UI exposes only `FULL` because the backend `mark_day_off` currently ignores `scope` (Plan-1 limitation). Expose `MORNING`/`AFTERNOON` once the backend honors scope.
- **Signal-mapping manager UI** (learnMapping/signalMappings): the CLI `aplan map` covers rule management; a visual manager is a later addition.
