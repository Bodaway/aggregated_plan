import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryPickerDialog } from './MemoryPickerDialog';
import type { Memory, ScoredMemory } from '@/lib/memory/types';

const target: Memory = {
  id: 'existing-1',
  kind: 'FACT',
  title: 'Le compte Witivio est refusé par le ServiceNow de Pernod',
  body: null,
  occurredAt: '2026-08-17T15:34:43Z',
  recordedAt: '2026-08-17T15:34:43Z',
  invalidatedAt: null,
  supersededBy: null,
  proposedSupersedes: null,
  status: 'ACTIVE',
  taskId: null,
  projectId: null,
  stakeholders: [],
};

const results: readonly ScoredMemory[] = [{ memory: target, score: 1.2 }];

const onSearch = vi.fn();
const onPick = vi.fn();
const onClose = vi.fn();

beforeEach(() => {
  onSearch.mockReset();
  onPick.mockReset();
  onClose.mockReset();
});

function open(props: Partial<React.ComponentProps<typeof MemoryPickerDialog>> = {}) {
  return render(
    <MemoryPickerDialog
      open
      heading="Merge into…"
      results={results}
      searched
      onSearch={onSearch}
      onPick={onPick}
      onClose={onClose}
      {...props}
    />
  );
}

describe('MemoryPickerDialog', () => {
  it('names what the pick is for', () => {
    open();

    expect(screen.getByText('Merge into…')).toBeInTheDocument();
  });

  it('searches the active memories for a target', () => {
    open({ results: [] });

    fireEvent.change(screen.getByPlaceholderText(/search the memory/i), {
      target: { value: 'witivio' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Search' }));

    expect(onSearch).toHaveBeenCalledWith('witivio');
  });

  it('reports the memory the user picked', () => {
    open();

    fireEvent.click(screen.getByRole('button', { name: target.title }));

    expect(onPick).toHaveBeenCalledWith('existing-1');
  });

  it('closes without picking anything', () => {
    open();

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(onClose).toHaveBeenCalled();
    expect(onPick).not.toHaveBeenCalled();
  });

  it('renders nothing while closed', () => {
    open({ open: false });

    expect(screen.queryByText('Merge into…')).not.toBeInTheDocument();
  });
});
