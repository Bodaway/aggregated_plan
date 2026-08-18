import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
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
