import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryBriefBar } from './MemoryBriefBar';
import type { Brief } from '@/lib/memory/types';

const brief: Brief = {
  date: '2026-08-18',
  pendingCount: 40,
  decisions: [
    {
      id: 'ab01710f',
      reference: 'm:ab0',
      title: 'Consolidation planifiée à 17h30',
      stakeholders: [],
      occurredOn: '2026-08-03',
    },
  ],
  decisionTotal: 1,
  commitments: [],
  commitmentTotal: 0,
  consolidation: { daysAgo: 1, stale: false },
};

describe('MemoryBriefBar', () => {
  it('leads with how many candidates await triage', () => {
    render(<MemoryBriefBar brief={brief} />);

    expect(screen.getByText(/40 to triage/i)).toBeInTheDocument();
  });

  it('shows the active decision and commitment totals', () => {
    render(<MemoryBriefBar brief={brief} />);

    expect(screen.getByText(/1 active decision/i)).toBeInTheDocument();
    expect(screen.getByText(/0 open commitments/i)).toBeInTheDocument();
  });

  it('reports how long ago the consolidation last ran', () => {
    render(<MemoryBriefBar brief={brief} />);

    expect(screen.getByText(/consolidated 1 day ago/i)).toBeInTheDocument();
  });

  it('warns when the consolidation has gone quiet', () => {
    render(
      <MemoryBriefBar brief={{ ...brief, consolidation: { daysAgo: 6, stale: true } }} />
    );

    expect(screen.getByText(/consolidation has gone quiet/i)).toBeInTheDocument();
  });

  it('says the consolidation never ran when it has no date', () => {
    render(
      <MemoryBriefBar brief={{ ...brief, consolidation: { daysAgo: null, stale: true } }} />
    );

    expect(screen.getByText(/never consolidated/i)).toBeInTheDocument();
  });
});
