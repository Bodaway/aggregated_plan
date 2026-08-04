import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';

import { TimesheetTimeline } from './TimesheetTimeline';
import type { AttributedBlock } from '@/hooks/use-timesheet';

// NOTE: bare local NaiveDateTime (no `Z`/offset) — exactly the wire format the backend
// emits — so new Date(iso).getHours() is deterministic in any test-runner timezone.
const blocks: AttributedBlock[] = [
  { startTime: '2026-06-08T08:00:00', endTime: '2026-06-08T10:00:00', gryzzlyProjectId: 'p1', kind: 'WORK', hours: 2, sourceRefs: [] },
  { startTime: '2026-06-08T09:00:00', endTime: '2026-06-08T10:00:00', gryzzlyProjectId: null, kind: 'MEETING', hours: 1, sourceRefs: [] },
  { startTime: '2026-06-08T14:00:00', endTime: '2026-06-08T16:00:00', gryzzlyProjectId: 'p1', kind: 'WORK', hours: 2, sourceRefs: [] },
];

describe('TimesheetTimeline', () => {
  it('renders morning and afternoon half-day columns', () => {
    render(<TimesheetTimeline blocks={blocks} />);
    expect(screen.getByText(/matin/i)).toBeInTheDocument();
    expect(screen.getByText(/après-midi/i)).toBeInTheDocument();
  });

  it('renders a bar per block that overlaps a half-day window', () => {
    const { container } = render(<TimesheetTimeline blocks={blocks} />);
    // 3 blocks all overlap their windows → at least 3 positioned bars.
    expect(container.querySelectorAll('[data-block]').length).toBeGreaterThanOrEqual(3);
  });

  it('shows an empty-state when there are no blocks', () => {
    render(<TimesheetTimeline blocks={[]} />);
    expect(screen.getByText(/aucune activité/i)).toBeInTheDocument();
  });
});
