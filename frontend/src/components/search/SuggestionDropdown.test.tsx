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
      ? [{ key: 'title', indices: titleIndices as [number, number][], value: t.title }]
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
    render(<SuggestionDropdown listboxId="lb" activeIndex={0} />);
    expect(screen.getAllByRole('option')).toHaveLength(2);
  });

  it('shows an empty-state status region (not listbox) when there are no matches', () => {
    ctx.query = 'zzzzz';
    ctx.matches = [];
    render(<SuggestionDropdown listboxId="lb" activeIndex={0} />);
    // Must NOT be a listbox — an empty listbox is an ARIA violation
    expect(screen.queryByRole('listbox')).toBeNull();
    // Must be a live region announcing the empty state
    const status = screen.getByRole('status');
    expect(status.textContent).toContain('zzzzz');
  });

  it('clicking a row opens the task and clears the query', () => {
    ctx.matches = [result(task('1', 'Auth work'))];
    render(<SuggestionDropdown listboxId="lb" activeIndex={0} />);
    fireEvent.mouseDown(screen.getAllByRole('option')[0]);
    expect(openSpy).toHaveBeenCalledWith('1');
    expect(clearSpy).toHaveBeenCalled();
  });

  it('renders the option at activeIndex with aria-selected="true"', () => {
    ctx.matches = [
      result(task('1', 'Auth A')),
      result(task('2', 'Auth B')),
    ];
    render(<SuggestionDropdown listboxId="lb" activeIndex={1} />);
    const options = screen.getAllByRole('option');
    expect(options[0]).toHaveAttribute('aria-selected', 'false');
    expect(options[1]).toHaveAttribute('aria-selected', 'true');
  });

  it('each option gets a stable id derived from listboxId', () => {
    ctx.matches = [
      result(task('1', 'Auth A')),
      result(task('2', 'Auth B')),
    ];
    render(<SuggestionDropdown listboxId="lb" activeIndex={0} />);
    const options = screen.getAllByRole('option');
    expect(options[0]).toHaveAttribute('id', 'lb-option-0');
    expect(options[1]).toHaveAttribute('id', 'lb-option-1');
  });

  it('bolds matched character ranges in the title', () => {
    ctx.matches = [result(task('1', 'Refactor auth middleware'), [[9, 12]])];
    render(<SuggestionDropdown listboxId="lb" activeIndex={0} />);
    expect(screen.getByText('auth').tagName).toBe('STRONG');
  });
});
