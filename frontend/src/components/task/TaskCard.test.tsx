import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { TaskCard, type TaskCardProps } from './TaskCard';

// StatusMenu calls useMutation from urql directly — provide a no-op mock so
// TaskCard renders without a urql Provider in tests.
vi.mock('urql', () => ({
  useMutation: () => [{ fetching: false, data: null, error: null }, vi.fn()],
  useQuery: () => [{ fetching: false, data: null, error: null }, vi.fn()],
}));

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

// TaskCardProps is flat — no nested `task` field.
// We spread TASK directly onto <TaskCard />.
const TASK: TaskCardProps = {
  id: '1',
  title: 'Example task',
  source: 'JIRA',
  sourceId: 'PROJ-1',
  status: 'TODO',
  urgency: 2,
  impact: 2,
  quadrant: 'Important',
  projectName: null,
  assignee: null,
  tags: [],
};

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

describe('TaskCard — isRecurring indicator', () => {
  it('shows the recurring icon when isRecurring=true', () => {
    const { container } = render(<TaskCard {...TASK} isRecurring={true} />);
    // The SVG has aria-label="Tâche récurrente" and role="img"
    const icon = container.querySelector('[aria-label="Tâche récurrente"]');
    expect(icon).not.toBeNull();
  });

  it('does NOT show the recurring icon when isRecurring=false', () => {
    const { container } = render(<TaskCard {...TASK} isRecurring={false} />);
    const icon = container.querySelector('[aria-label="Tâche récurrente"]');
    expect(icon).toBeNull();
  });

  it('does NOT show the recurring icon when isRecurring is omitted', () => {
    const { container } = render(<TaskCard {...TASK} />);
    const icon = container.querySelector('[aria-label="Tâche récurrente"]');
    expect(icon).toBeNull();
  });
});

function renderCard(overrides: Partial<TaskCardProps> = {}) {
  return render(<TaskCard {...TASK} {...overrides} />);
}

function renderCardCompact(overrides: Partial<TaskCardProps> = {}) {
  return render(<TaskCard {...TASK} compact {...overrides} />);
}

describe('TaskCard — delegatedTo badge', () => {
  it('shows the delegate name when delegatedTo is set (full card)', () => {
    renderCard({ delegatedTo: 'Marie' });
    expect(screen.getByText('→ Marie')).toBeInTheDocument();
  });

  it('shows no delegate badge when delegatedTo is absent (full card)', () => {
    renderCard({});
    expect(screen.queryByText(/^→ /)).not.toBeInTheDocument();
  });

  it('shows the delegate name when delegatedTo is set (compact card)', () => {
    renderCardCompact({ delegatedTo: 'Marie' });
    expect(screen.getByText('→ Marie')).toBeInTheDocument();
  });

  it('shows no delegate badge when delegatedTo is absent (compact card)', () => {
    renderCardCompact({});
    expect(screen.queryByText(/^→ /)).not.toBeInTheDocument();
  });
});

describe('TaskCard highlight', () => {
  it('renders without ring or dim when search is inactive', () => {
    const { container } = render(<TaskCard {...TASK} />);
    const root = container.querySelector('[data-testid="task-card-root"]');
    expect(root?.className).not.toMatch(/ring-/);
    expect(root?.className).not.toMatch(/opacity-/);
  });

  it('adds ring classes when the task matches', () => {
    ctx.highlightActive = true;
    ctx.matchedIds = new Set(['1']);
    const { container } = render(<TaskCard {...TASK} />);
    const root = container.querySelector('[data-testid="task-card-root"]');
    expect(root?.className).toMatch(/ring-2/);
    expect(root?.className).toMatch(/ring-blue-500/);
  });

  it('adds dim classes when search is active and the task does NOT match', () => {
    ctx.highlightActive = true;
    ctx.matchedIds = new Set(['2']); // some other id
    const { container } = render(<TaskCard {...TASK} />);
    const root = container.querySelector('[data-testid="task-card-root"]');
    expect(root?.className).toMatch(/opacity-40/);
  });

  it('names the task it renders, so a text selection inside it can be attributed', () => {
    const { container } = renderCard();

    expect(container.querySelector(`[data-task-id="${TASK.id}"]`)).not.toBeNull();
  });

  it('names the task in the compact variant too', () => {
    const { container } = renderCard({ compact: true });

    expect(container.querySelector(`[data-task-id="${TASK.id}"]`)).not.toBeNull();
  });

  it('does not open the task when the click merely completes a text selection', () => {
    const onClick = vi.fn();
    const original = window.getSelection;
    window.getSelection = () => ({ toString: () => 'un fragment sélectionné' }) as unknown as Selection;

    const { container } = renderCard({ onClick });
    const root = container.querySelector('[data-testid="task-card-root"]') as HTMLElement;
    root.click();

    window.getSelection = original;
    expect(onClick).not.toHaveBeenCalled();
  });

  it('still opens the task on a plain click', () => {
    const onClick = vi.fn();
    const original = window.getSelection;
    window.getSelection = () => ({ toString: () => '' }) as unknown as Selection;

    const { container } = renderCard({ onClick });
    const root = container.querySelector('[data-testid="task-card-root"]') as HTMLElement;
    root.click();

    window.getSelection = original;
    expect(onClick).toHaveBeenCalled();
  });
});

// ─── Overdue (R74/R75) ───────────────────────────────────────────────────────
//
// The delay is a read-time qualification the server hands down: the card only
// paints it. Two signals must survive together on the same element — the thick
// left border keeps coding urgency, the delay adds a tint, a ring and a pill.

function rootOf(container: HTMLElement): HTMLElement {
  return container.querySelector('[data-testid="task-card-root"]') as HTMLElement;
}

describe('TaskCard — compact deadline (R75)', () => {
  it('paints the deadline it receives, so the -Nj pill has a referent', () => {
    renderCardCompact({ deadline: '2026-08-20' });

    expect(screen.getByText('2026-08-20')).toBeInTheDocument();
  });

  it('paints no deadline line when the task has none', () => {
    renderCardCompact({ deadline: null });

    expect(screen.queryByText(/^\d{4}-\d{2}-\d{2}$/)).not.toBeInTheDocument();
  });

  it('uses the same calendar icon as the full rendering', () => {
    const iconOf = (root: HTMLElement) =>
      within(root).getByText('2026-08-20').querySelector('svg path')?.getAttribute('d');

    const { container: compact } = renderCardCompact({ deadline: '2026-08-20' });
    const { container: full } = renderCard({ deadline: '2026-08-20' });

    expect(iconOf(compact)).toBeTruthy();
    expect(iconOf(compact)).toBe(iconOf(full));
  });
});

describe('TaskCard — overdue marker, compact rendering', () => {
  it('shows the red treatment and the age pill for a broken deadline', () => {
    const { container } = renderCardCompact({ overdueKind: 'DEADLINE', overdueDays: 5 });

    expect(screen.getByTestId('overdue-badge')).toHaveTextContent('⚠ -5j');
    expect(rootOf(container).className).toMatch(/bg-red-50/);
    expect(rootOf(container).className).toMatch(/ring-2 ring-red-400/);
  });

  it('shows the amber treatment and the age pill for a planning slip', () => {
    const { container } = renderCardCompact({ overdueKind: 'PLANNED', overdueDays: 12 });

    expect(screen.getByTestId('overdue-badge')).toHaveTextContent('⚠ -12j');
    expect(rootOf(container).className).toMatch(/bg-amber-50/);
    expect(rootOf(container).className).toMatch(/ring-2 ring-amber-400/);
  });

  it('replaces the white surface rather than stacking on it', () => {
    const { container } = renderCardCompact({ overdueKind: 'DEADLINE', overdueDays: 1 });

    expect(rootOf(container).className).not.toMatch(/bg-white/);
  });

  it('carries the French tooltip naming the level and the age', () => {
    renderCardCompact({ overdueKind: 'DEADLINE', overdueDays: 5 });

    expect(screen.getByTestId('overdue-badge')).toHaveAttribute('title', 'Échéance dépassée de 5 jours');
  });

  it('paints nothing when the task is on time', () => {
    const { container } = renderCardCompact({ overdueKind: 'NONE', overdueDays: null });

    expect(screen.queryByTestId('overdue-badge')).not.toBeInTheDocument();
    expect(rootOf(container).className).toMatch(/bg-white/);
    expect(rootOf(container).className).not.toMatch(/ring-/);
  });

  it('paints nothing when the caller does not select the delay at all', () => {
    // Absent means "not asked for", never "unknown": the card reads as on time.
    const { container } = renderCardCompact({});

    expect(screen.queryByTestId('overdue-badge')).not.toBeInTheDocument();
    expect(rootOf(container).className).toMatch(/bg-white/);
  });
});

describe('TaskCard — overdue marker, full rendering', () => {
  it('shows the red treatment and the age pill for a broken deadline', () => {
    const { container } = renderCard({ overdueKind: 'DEADLINE', overdueDays: 3 });

    expect(screen.getByTestId('overdue-badge')).toHaveTextContent('⚠ -3j');
    expect(rootOf(container).className).toMatch(/bg-red-50/);
    expect(rootOf(container).className).toMatch(/ring-2 ring-red-400/);
  });

  it('shows the amber treatment for a planning slip', () => {
    const { container } = renderCard({ overdueKind: 'PLANNED', overdueDays: 2 });

    expect(screen.getByTestId('overdue-badge')).toHaveTextContent('⚠ -2j');
    expect(rootOf(container).className).toMatch(/bg-amber-50/);
  });

  it('paints nothing when the task is on time', () => {
    const { container } = renderCard({ overdueKind: 'NONE', overdueDays: null });

    expect(screen.queryByTestId('overdue-badge')).not.toBeInTheDocument();
    expect(rootOf(container).className).toMatch(/bg-white/);
  });
});

describe('TaskCard — the urgency border survives the overdue ring (§23.5)', () => {
  // The explicit design constraint: the delay layers *on top of* urgency, it
  // never replaces it. Both dimensions must remain readable at once.
  it('keeps the critical border while ringing a broken deadline', () => {
    const { container } = renderCardCompact({ urgency: 4, overdueKind: 'DEADLINE', overdueDays: 5 });

    expect(rootOf(container).className).toMatch(/border-l-red-600/);
    expect(rootOf(container).className).toMatch(/ring-red-400/);
  });

  it('keeps a low-urgency border under a red ring — the two are independent', () => {
    // A medium-urgency task with a blown deadline: yellow border, red ring.
    // If the ring were derived from urgency, or replaced the border, this fails.
    const { container } = renderCardCompact({ urgency: 2, overdueKind: 'DEADLINE', overdueDays: 8 });

    expect(rootOf(container).className).toMatch(/border-l-yellow-600/);
    expect(rootOf(container).className).toMatch(/ring-red-400/);
  });

  it('keeps a critical border under an amber ring', () => {
    const { container } = renderCardCompact({ urgency: 4, overdueKind: 'PLANNED', overdueDays: 1 });

    expect(rootOf(container).className).toMatch(/border-l-red-600/);
    expect(rootOf(container).className).toMatch(/ring-amber-400/);
  });

  it('keeps the border-l-4 width so the urgency stripe stays visible', () => {
    const { container } = renderCardCompact({ urgency: 3, overdueKind: 'DEADLINE', overdueDays: 2 });

    expect(rootOf(container).className).toMatch(/border-l-4/);
    expect(rootOf(container).className).toMatch(/border-l-orange-600/);
  });

  it('keeps the urgency border in the full rendering too', () => {
    const { container } = renderCard({ urgency: 1, overdueKind: 'PLANNED', overdueDays: 4 });

    expect(rootOf(container).className).toMatch(/border-l-gray-400/);
    expect(rootOf(container).className).toMatch(/ring-amber-400/);
  });
});

describe('TaskCard — overdue ring vs search ring', () => {
  it('yields the ring to the transient search highlight, keeping the tint', () => {
    // Two `ring-*` utilities on one element fight over a single CSS variable;
    // the search ring is transient, so it wins — the tint still marks the delay.
    ctx.highlightActive = true;
    ctx.matchedIds = new Set(['1']);
    const { container } = renderCardCompact({ overdueKind: 'DEADLINE', overdueDays: 5 });

    expect(rootOf(container).className).toMatch(/ring-blue-500/);
    expect(rootOf(container).className).not.toMatch(/ring-red-400/);
    expect(rootOf(container).className).toMatch(/bg-red-50/);
    expect(screen.getByTestId('overdue-badge')).toBeInTheDocument();
  });

  it('keeps its own ring when search is active but the task does not match', () => {
    ctx.highlightActive = true;
    ctx.matchedIds = new Set(['other']);
    const { container } = renderCardCompact({ overdueKind: 'DEADLINE', overdueDays: 5 });

    expect(rootOf(container).className).toMatch(/ring-red-400/);
  });
});
