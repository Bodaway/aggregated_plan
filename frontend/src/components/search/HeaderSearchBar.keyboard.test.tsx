/**
 * Keyboard navigation tests for HeaderSearchBar.
 * Uses a direct useSearch mock (same pattern as SuggestionDropdown.test.tsx)
 * so we can inject matches without needing a real Fuse index.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { FuseResult } from 'fuse.js';
import { HeaderSearchBar } from './HeaderSearchBar';
import type { SearchableTask } from '@/lib/search/types';

interface MockCtx {
  query: string;
  setQuery: (q: string) => void;
  matches: FuseResult<SearchableTask>[];
  matchedIds: ReadonlySet<string>;
  highlightActive: boolean;
  openTaskId: string | null;
  openTaskInSheet: (id: string) => void;
  closeSheet: () => void;
  clearQuery: () => void;
  loading: boolean;
  error: Error | null;
}

let ctx: MockCtx;
vi.mock('@/lib/search/SearchProvider', () => ({
  useSearch: () => ctx,
}));

function task(id: string, title: string): SearchableTask {
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
  };
}

function result(t: SearchableTask): FuseResult<SearchableTask> {
  return { item: t, refIndex: 0, matches: [] };
}

const openSpy = vi.fn();
const clearSpy = vi.fn();

beforeEach(() => {
  openSpy.mockClear();
  clearSpy.mockClear();
  ctx = {
    query: 'auth',
    setQuery: vi.fn(),
    matches: [result(task('1', 'Auth A')), result(task('2', 'Auth B'))],
    matchedIds: new Set(['1', '2']),
    highlightActive: true,
    openTaskId: null,
    openTaskInSheet: openSpy,
    closeSheet: vi.fn(),
    clearQuery: clearSpy,
    loading: false,
    error: null,
  };
});

describe('HeaderSearchBar keyboard navigation', () => {
  it('ArrowDown on the input advances aria-activedescendant to the next option', () => {
    render(<HeaderSearchBar />);
    const input = screen.getByRole('combobox') as HTMLInputElement;

    // Focus to open the dropdown (highlightActive=true + isFocused=true)
    fireEvent.focus(input);
    // Initially points to option 0
    const firstId = input.getAttribute('aria-activedescendant')!;
    expect(firstId).toMatch(/-option-0$/);

    // Arrow down → option 1
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    const secondId = input.getAttribute('aria-activedescendant')!;
    expect(secondId).toMatch(/-option-1$/);
  });

  it('ArrowUp on the input moves back towards option 0', () => {
    render(<HeaderSearchBar />);
    const input = screen.getByRole('combobox') as HTMLInputElement;
    fireEvent.focus(input);

    // Advance to index 1
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(input.getAttribute('aria-activedescendant')).toMatch(/-option-1$/);

    // Go back to index 0
    fireEvent.keyDown(input, { key: 'ArrowUp' });
    expect(input.getAttribute('aria-activedescendant')).toMatch(/-option-0$/);
  });

  it('Enter picks the active match and clears the query', () => {
    render(<HeaderSearchBar />);
    const input = screen.getByRole('combobox') as HTMLInputElement;
    fireEvent.focus(input);

    // Active index is 0 → task id '1'
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(openSpy).toHaveBeenCalledWith('1');
    expect(clearSpy).toHaveBeenCalled();
  });

  it('ArrowDown clamps at the last option', () => {
    render(<HeaderSearchBar />);
    const input = screen.getByRole('combobox') as HTMLInputElement;
    fireEvent.focus(input);

    // Two matches → max index is 1
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    // Should still be at index 1 (clamped)
    expect(input.getAttribute('aria-activedescendant')).toMatch(/-option-1$/);
  });

  it('does not set aria-activedescendant when there are no matches', () => {
    ctx.matches = [];
    render(<HeaderSearchBar />);
    const input = screen.getByRole('combobox') as HTMLInputElement;
    fireEvent.focus(input);

    expect(input).not.toHaveAttribute('aria-activedescendant');
    // Should not throw
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(input).not.toHaveAttribute('aria-activedescendant');
  });
});
