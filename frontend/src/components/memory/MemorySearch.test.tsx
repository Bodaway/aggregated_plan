import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemorySearch } from './MemorySearch';
import type { Memory, ScoredMemory } from '@/lib/memory/types';

const base: Memory = {
  id: 'm-1',
  kind: 'DECISION',
  title: 'Consolidation planifiée à 17h30, pas 22h',
  body: 'Le poste est éteint le soir.',
  occurredAt: '2026-08-03T10:00:00Z',
  recordedAt: '2026-08-03T10:00:00Z',
  invalidatedAt: null,
  supersededBy: null,
  proposedSupersedes: null,
  status: 'ACTIVE',
  taskId: null,
  projectId: null,
  stakeholders: [],
};

const hit = (over: Partial<Memory>, score = 1.42): ScoredMemory => ({
  memory: { ...base, ...over },
  score,
});

const onSearch = vi.fn();

beforeEach(() => onSearch.mockReset());

describe('MemorySearch', () => {
  it('searches the typed query with history off by default', () => {
    render(<MemorySearch results={[]} searched={false} onSearch={onSearch} />);

    fireEvent.change(screen.getByPlaceholderText(/search the memory/i), {
      target: { value: 'consolidation' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Search' }));

    expect(onSearch).toHaveBeenCalledWith('consolidation', false);
  });

  it('includes history when the user asks for it', () => {
    render(<MemorySearch results={[]} searched={false} onSearch={onSearch} />);

    fireEvent.change(screen.getByPlaceholderText(/search the memory/i), {
      target: { value: 'consolidation' },
    });
    fireEvent.click(screen.getByLabelText(/include history/i));
    fireEvent.click(screen.getByRole('button', { name: 'Search' }));

    expect(onSearch).toHaveBeenCalledWith('consolidation', true);
  });

  it('shows a hit with its relevance score', () => {
    render(<MemorySearch results={[hit({})]} searched onSearch={onSearch} />);

    expect(screen.getByText(base.title)).toBeInTheDocument();
    expect(screen.getByText('1.42')).toBeInTheDocument();
  });

  it('marks a hit that is no longer true and names what replaced it', () => {
    render(
      <MemorySearch
        results={[hit({ invalidatedAt: '2026-08-18T08:18:01Z', supersededBy: 'm-9' })]}
        searched
        onSearch={onSearch}
      />
    );

    expect(screen.getByText(/no longer true/i)).toBeInTheDocument();
    expect(screen.getByText(/m-9/)).toBeInTheDocument();
  });

  it('badges a hit still waiting in the validation queue', () => {
    render(<MemorySearch results={[hit({ status: 'PENDING' })]} searched onSearch={onSearch} />);

    expect(screen.getByText(/awaiting validation/i)).toBeInTheDocument();
  });

  it('says nothing matched once a search came back empty', () => {
    render(<MemorySearch results={[]} searched onSearch={onSearch} />);

    expect(screen.getByText(/no memory matched/i)).toBeInTheDocument();
  });
});
