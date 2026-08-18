import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { Brief, Memory, ScoredMemory } from '@/lib/memory/types';

const candidate: Memory = {
  id: 'cand-1',
  kind: 'FACT',
  title: 'Le compte Witivio est refusé par le ServiceNow de Pernod Ricard',
  body: 'Constaté depuis le profil dédié.',
  occurredAt: '2026-08-17T15:34:43Z',
  recordedAt: '2026-08-17T15:34:43Z',
  invalidatedAt: null,
  supersededBy: null,
  proposedSupersedes: null,
  status: 'PENDING',
  taskId: null,
  projectId: null,
  stakeholders: [],
};

const existing: Memory = { ...candidate, id: 'existing-1', title: 'Une mémoire déjà active', status: 'ACTIVE' };

const brief: Brief = {
  date: '2026-08-18',
  pendingCount: 40,
  decisions: [],
  decisionTotal: 1,
  commitments: [],
  commitmentTotal: 0,
  consolidation: { daysAgo: 1, stale: false },
};

const queue = {
  pending: [candidate] as readonly Memory[],
  brief: brief as Brief | null,
  loading: false,
  error: null as string | null,
  busy: false,
  nearDuplicates: {} as Record<string, readonly Memory[]>,
  accept: vi.fn(),
  forceAccept: vi.fn(),
  reject: vi.fn(),
  mergeInto: vi.fn(),
  supersede: vi.fn(),
  remember: vi.fn(),
  importDirectory: vi.fn(),
  importReport: null,
  importing: false,
};

const recall = {
  results: [{ memory: existing, score: 1.1 }] as readonly ScoredMemory[],
  searched: true,
  loading: false,
  error: null as string | null,
  search: vi.fn(),
};

vi.mock('@/hooks/use-memory', () => ({
  useMemoryQueue: () => queue,
  useMemoryRecall: () => recall,
}));

import { MemoryPage } from './MemoryPage';

beforeEach(() => {
  queue.pending = [candidate];
  queue.nearDuplicates = {};
  [queue.accept, queue.forceAccept, queue.reject, queue.mergeInto, queue.supersede, queue.remember, queue.importDirectory, recall.search].forEach(
    fn => fn.mockReset()
  );
});

describe('MemoryPage', () => {
  it('leads with the queue size from the brief', () => {
    render(<MemoryPage />);

    expect(screen.getByText(/40 to triage/i)).toBeInTheDocument();
  });

  it('renders one card per candidate awaiting a verdict', () => {
    render(<MemoryPage />);

    expect(screen.getByText(candidate.title)).toBeInTheDocument();
  });

  it('says so when nothing awaits triage', () => {
    queue.pending = [];
    render(<MemoryPage />);

    expect(screen.getByText(/nothing to triage/i)).toBeInTheDocument();
  });

  it('accepts a candidate through the queue hook', () => {
    render(<MemoryPage />);

    fireEvent.click(screen.getByRole('button', { name: 'Keep' }));

    expect(queue.accept).toHaveBeenCalledWith('cand-1');
  });

  it('merges into the memory picked in the dialog', () => {
    render(<MemoryPage />);

    fireEvent.click(screen.getByRole('button', { name: 'Merge…' }));
    fireEvent.click(screen.getByRole('button', { name: existing.title }));

    expect(queue.mergeInto).toHaveBeenCalledWith('cand-1', 'existing-1');
  });

  it('lets the backend resolve the target when the candidate names one', () => {
    queue.pending = [{ ...candidate, proposedSupersedes: 'older-1' }];
    render(<MemoryPage />);

    fireEvent.click(screen.getByRole('button', { name: 'Replace…' }));

    expect(queue.supersede).toHaveBeenCalledWith('cand-1', null);
  });

  it('imports the directory the panel holds', () => {
    render(<MemoryPage />);

    fireEvent.click(screen.getByRole('button', { name: 'Import' }));

    expect(queue.importDirectory).toHaveBeenCalledWith(
      expect.stringContaining('/memory')
    );
  });

  it('records a memory typed by hand', () => {
    render(<MemoryPage />);

    fireEvent.click(screen.getByRole('button', { name: /new memory/i }));
    fireEvent.change(screen.getByLabelText('Title'), { target: { value: 'Un fait noté à la main' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(queue.remember).toHaveBeenCalledWith(
      expect.objectContaining({ title: 'Un fait noté à la main', confirmed: false })
    );
  });
});
