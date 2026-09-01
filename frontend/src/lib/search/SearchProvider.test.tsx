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

  // openTaskInSheet awaits the outgoing panel flush before it switches, so the
  // state change can land a microtask late — drive it through an async act.
  it('openTaskInSheet + closeSheet drive openTaskId', async () => {
    const { getCtx } = renderWithProvider();
    expect(getCtx().openTaskId).toBeNull();
    await act(async () => { getCtx().openTaskInSheet('1'); });
    expect(getCtx().openTaskId).toBe('1');
    await act(async () => { getCtx().closeSheet(); });
    expect(getCtx().openTaskId).toBeNull();
  });
});
