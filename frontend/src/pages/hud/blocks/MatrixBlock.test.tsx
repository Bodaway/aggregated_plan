import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Read directly, same technique the other block tests use — jsdom does not
// apply this stylesheet, so a rendered element's computed style cannot tell
// us whether an empty state is styled as deliberate or merely present.
const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

const matrixMock = vi.fn();
vi.mock('@/hooks/use-priority-matrix', () => ({
  usePriorityMatrix: (...args: unknown[]) => matrixMock(...args),
}));

import { MatrixBlock } from './MatrixBlock';

function task(over: Record<string, unknown> = {}) {
  return {
    id: 't',
    title: 'Task',
    status: 'TODO',
    urgency: 'HIGH',
    impact: 'HIGH',
    deadline: null,
    project: null,
    isRecurring: false,
    recurrenceId: null,
    occurrenceDate: null,
    ...over,
  };
}

function mockMatrix(over: Record<string, unknown> = {}) {
  matrixMock.mockReturnValue({
    data: { urgentImportant: [], important: [], urgent: [], neither: [], ...over },
  });
}

describe('MatrixBlock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-11T09:00:00'));
    matrixMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it('lists the top of the matrix, criticals first and marked', () => {
    mockMatrix({
      urgentImportant: [task({ id: 'q1', title: 'Revue eProject A3' })],
      neither: [task({ id: 'crit', title: 'Prod down', urgency: 'CRITICAL' })],
    });

    render(<MatrixBlock lit={false} />);

    expect(screen.getByTestId('matrix-block')).toBeInTheDocument();
    expect(screen.getByText(/2 to do/i)).toBeInTheDocument();

    const rows = screen.getAllByTestId('matrix-task');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('Prod down');
    expect(rows[1]).toHaveTextContent('Revue eProject A3');

    const dots = screen.getAllByTestId('matrix-dot');
    expect(dots[0].className).toContain('hud-matrix__dot--critical');
    expect(dots[1].className).not.toContain('hud-matrix__dot--critical');
  });

  it('caps the rendered rows at five while the label keeps the true total', () => {
    // The same ruling every other capped block in this HUD follows
    // (MAX_VISIBLE_DEADLINES, MAX_VISIBLE_MODELS): the panel has a fixed
    // grid area, so what is drawn is capped — but the label must not lie
    // about how much is waiting.
    mockMatrix({
      urgentImportant: Array.from({ length: 8 }, (_, i) => task({ id: `t${i}`, title: `Task ${i}` })),
    });

    render(<MatrixBlock lit={false} />);

    expect(screen.getByText(/8 to do/i)).toBeInTheDocument();
    expect(screen.getAllByTestId('matrix-task')).toHaveLength(5);
  });

  it('reads a deliberate empty state when the top of the matrix is clear', () => {
    mockMatrix();

    render(<MatrixBlock lit={false} />);

    expect(screen.getByText(/nothing at the top of the matrix/i)).toBeInTheDocument();
    expect(screen.queryAllByTestId('matrix-task')).toHaveLength(0);

    // Presence in the DOM is not legibility — assert the stylesheet actually
    // marks this as a deliberate empty state, same guard as Pressure's.
    const emptyRule = HUD_CSS.match(/\.hud-matrix__empty\s*\{[^}]*\}/)?.[0] ?? '';
    expect(emptyRule).toMatch(/font-style:\s*italic/);
  });

  it('survives the matrix not having loaded yet', () => {
    matrixMock.mockReturnValue({ data: null });

    render(<MatrixBlock lit={false} />);

    expect(screen.getByTestId('matrix-block')).toBeInTheDocument();
    expect(screen.getByText(/nothing at the top of the matrix/i)).toBeInTheDocument();
  });

  it('shows a deadline when the task has one, and nothing when it has not', () => {
    // Measured on this database, 8 of 54 open followed tasks carry a
    // deadline — so the column is the exception, not the rule, and an
    // undated task must render no cell rather than a placeholder.
    vi.setSystemTime(new Date('2026-09-11T09:00:00'));
    mockMatrix({
      urgentImportant: [
        task({ id: 'dated', title: 'Saft cadrage : CI/CD', deadline: '2026-09-11' }),
        task({ id: 'undated', title: 'Clé USB de secours', deadline: null }),
      ],
    });

    render(<MatrixBlock lit={false} />);

    const rows = screen.getAllByTestId('matrix-task');
    expect(rows[0]).toHaveTextContent('Today');
    expect(rows[1]).toHaveTextContent('Clé USB de secours');
    expect(rows[1].querySelector('b')).toBeNull();
  });

  it('leaves the deadline in the muted ink even when it falls today', () => {
    // Pink is Pressure's one reserved signal for "due today". Spending it
    // here too would have two panels a hand's width apart shouting the same
    // thing about the same task.
    vi.setSystemTime(new Date('2026-09-11T09:00:00'));
    mockMatrix({ urgentImportant: [task({ id: 'hot', deadline: '2026-09-11' })] });

    render(<MatrixBlock lit={false} />);

    expect(screen.getByTestId('matrix-task').className).not.toContain('hot');
    const rowRule = HUD_CSS.match(/\.hud-matrix__row b\s*\{[^}]*\}/)?.[0] ?? '';
    expect(rowRule).not.toMatch(/--cn-red/);
  });

  it('says what it is not showing, so a short list does not read as an empty backlog', () => {
    // The block shows one quadrant plus the criticals. Four rows on screen
    // with 43 tasks below the line is a very different day from four rows
    // with none, and the panel must not make them look the same.
    mockMatrix({
      urgentImportant: [task({ id: 'q1' })],
      important: [task({ id: 'i1' }), task({ id: 'i2' })],
      urgent: [task({ id: 'u1' })],
      neither: Array.from({ length: 43 }, (_, i) => task({ id: `n${i}` })),
    });

    render(<MatrixBlock lit={false} />);

    const backlog = screen.getByTestId('matrix-backlog');
    expect(backlog).toHaveTextContent('Important 2');
    expect(backlog).toHaveTextContent('Urgent 1');
    expect(backlog).toHaveTextContent('Neither 43');
  });

  it('keeps the backlog line on an empty matrix rather than hiding the panel’s floor', () => {
    mockMatrix();

    render(<MatrixBlock lit={false} />);

    const backlog = screen.getByTestId('matrix-backlog');
    expect(backlog).toHaveTextContent('Important 0');
    expect(backlog).toHaveTextContent('Neither 0');
  });

  it('wears the HUD’s one glow only while it is the dominant block', () => {
    mockMatrix();

    const { rerender } = render(<MatrixBlock lit />);
    expect(screen.getByTestId('matrix-block').className).toContain('hud-panel--lit');

    rerender(<MatrixBlock lit={false} />);
    expect(screen.getByTestId('matrix-block').className).not.toContain('hud-panel--lit');
  });
});
