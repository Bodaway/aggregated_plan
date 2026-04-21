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
});
