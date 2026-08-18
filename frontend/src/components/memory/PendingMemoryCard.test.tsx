import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { PendingMemoryCard } from './PendingMemoryCard';
import type { Memory } from '@/lib/memory/types';

const candidate: Memory = {
  id: 'cand-1',
  kind: 'FACT',
  title: 'Le compte Witivio est refusé par le ServiceNow de Pernod Ricard',
  body: 'Constaté en explorant le catalogue depuis le profil dédié.',
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

const duplicate: Memory = {
  ...candidate,
  id: 'existing-1',
  title: 'Le compte Witivio est refusé par le ServiceNow de Pernod',
  status: 'ACTIVE',
};

const handlers = {
  onAccept: vi.fn(),
  onForceAccept: vi.fn(),
  onReject: vi.fn(),
  onMerge: vi.fn(),
  onMergeInto: vi.fn(),
  onSupersede: vi.fn(),
};

beforeEach(() => {
  Object.values(handlers).forEach(fn => fn.mockReset());
});

describe('PendingMemoryCard', () => {
  it('shows the kind, the date and the title', () => {
    render(<PendingMemoryCard memory={candidate} {...handlers} />);

    expect(screen.getByText('fact')).toBeInTheDocument();
    expect(screen.getByText('17/08')).toBeInTheDocument();
    expect(screen.getByText(candidate.title)).toBeInTheDocument();
  });

  it('shows the body, which is the context the verdict is judged on', () => {
    render(<PendingMemoryCard memory={candidate} {...handlers} />);

    expect(screen.getByText(candidate.body as string)).toBeInTheDocument();
  });

  it('accepts the candidate when Keep is clicked', () => {
    render(<PendingMemoryCard memory={candidate} {...handlers} />);

    fireEvent.click(screen.getByRole('button', { name: 'Keep' }));

    expect(handlers.onAccept).toHaveBeenCalledWith('cand-1');
  });

  it('rejects the candidate when Discard is clicked', () => {
    render(<PendingMemoryCard memory={candidate} {...handlers} />);

    fireEvent.click(screen.getByRole('button', { name: 'Discard' }));

    expect(handlers.onReject).toHaveBeenCalledWith('cand-1');
  });

  it('names the near-duplicate the backend refused the accept on', () => {
    render(
      <PendingMemoryCard memory={candidate} nearDuplicates={[duplicate]} {...handlers} />
    );

    expect(screen.getByText(/looks like an existing memory/i)).toBeInTheDocument();
    expect(screen.getByText(duplicate.title)).toBeInTheDocument();
  });

  it('folds the candidate into the named duplicate on Merge into it', () => {
    render(
      <PendingMemoryCard memory={candidate} nearDuplicates={[duplicate]} {...handlers} />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Merge into it' }));

    expect(handlers.onMergeInto).toHaveBeenCalledWith('cand-1', 'existing-1');
  });

  it('forces the accept through when the user says Add anyway', () => {
    render(
      <PendingMemoryCard memory={candidate} nearDuplicates={[duplicate]} {...handlers} />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Add anyway' }));

    expect(handlers.onForceAccept).toHaveBeenCalledWith('cand-1');
  });

  it('flags a candidate that proposes to replace an existing memory', () => {
    render(
      <PendingMemoryCard
        memory={{ ...candidate, proposedSupersedes: 'older-1' }}
        {...handlers}
      />
    );

    expect(screen.getByText(/replaces an existing memory/i)).toBeInTheDocument();
  });
});
