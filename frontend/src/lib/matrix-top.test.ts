import { describe, it, expect } from 'vitest';
import { matrixBacklog, topOfMatrix, type MatrixCandidateTask, type MatrixQuadrants } from './matrix-top';

function task(over: Partial<MatrixCandidateTask> = {}): MatrixCandidateTask {
  return {
    id: 't',
    title: 'Task',
    status: 'TODO',
    urgency: 3,
    impact: 3,
    deadline: null,
    project: null,
    isRecurring: false,
    recurrenceId: null,
    occurrenceDate: null,
    ...over,
  };
}

function quadrants(over: Partial<MatrixQuadrants> = {}): MatrixQuadrants {
  return { urgentImportant: [], important: [], urgent: [], neither: [], ...over };
}

describe('topOfMatrix', () => {
  it('returns nothing before the matrix has loaded', () => {
    expect(topOfMatrix(null)).toEqual([]);
  });

  it('puts every critical task ahead of the urgent-and-important quadrant', () => {
    // Critical is urgency 4 wherever it sits: the priority page pulls it out
    // of all four quadrants into its own band above the grid, and this is
    // the same band. A critical task parked in `neither` still outranks an
    // ordinary Q1 task.
    const data = quadrants({
      urgentImportant: [task({ id: 'q1', title: 'Revue eProject A3' })],
      neither: [task({ id: 'crit', title: 'Prod down', urgency: 4 })],
    });

    expect(topOfMatrix(data).map((t) => t.id)).toEqual(['crit', 'q1']);
    expect(topOfMatrix(data)[0].critical).toBe(true);
    expect(topOfMatrix(data)[1].critical).toBe(false);
  });

  it('keeps only the urgent-and-important quadrant out of the three non-critical ones', () => {
    // "The top of the matrix" is one quadrant, not a ranking of all four:
    // important-not-urgent, urgent-not-important and neither are what the
    // block deliberately does NOT show.
    const data = quadrants({
      urgentImportant: [task({ id: 'q1' })],
      important: [task({ id: 'q2' })],
      urgent: [task({ id: 'q3' })],
      neither: [task({ id: 'q4' })],
    });

    expect(topOfMatrix(data).map((t) => t.id)).toEqual(['q1']);
  });

  it('orders each band by deadline proximity, undated last', () => {
    const data = quadrants({
      urgentImportant: [
        task({ id: 'later', deadline: '2026-09-20' }),
        task({ id: 'undated', deadline: null }),
        task({ id: 'sooner', deadline: '2026-09-11' }),
      ],
    });

    expect(topOfMatrix(data).map((t) => t.id)).toEqual(['sooner', 'later', 'undated']);
  });

  it('drops done and cancelled tasks, keeps blocked ones', () => {
    // Same verdict as every other surface in this app (`isTaskOpen`):
    // stalled is not finished, and a finished task is not something to do.
    const data = quadrants({
      urgentImportant: [
        task({ id: 'done', status: 'DONE' }),
        task({ id: 'cancelled', status: 'CANCELLED' }),
        task({ id: 'blocked', status: 'BLOCKED' }),
      ],
    });

    expect(topOfMatrix(data).map((t) => t.id)).toEqual(['blocked']);
  });

  it('shows one row per recurring series, the earliest occurrence', () => {
    // A weekly template with four upcoming occurrences would otherwise fill
    // the whole block with the same title four times over — the priority
    // page collapses them for exactly this reason.
    const data = quadrants({
      urgentImportant: [
        task({ id: 'occ-2', title: 'Point hebdo', isRecurring: true, recurrenceId: 'r1', occurrenceDate: '2026-09-18' }),
        task({ id: 'occ-1', title: 'Point hebdo', isRecurring: true, recurrenceId: 'r1', occurrenceDate: '2026-09-11' }),
        task({ id: 'plain', title: 'Autre' }),
      ],
    });

    const ids = topOfMatrix(data).map((t) => t.id);
    expect(ids).toContain('occ-1');
    expect(ids).not.toContain('occ-2');
    expect(ids).toHaveLength(2);
  });

  it('reads urgency as the GraphQL string enum as well as a number', () => {
    // `priorityMatrix` returns UrgencyLevelGql, not Int — the priority page
    // carries the same conversion, and a block that only understood numbers
    // would silently classify every critical task as ordinary.
    const data = quadrants({
      urgentImportant: [task({ id: 'crit', urgency: 'CRITICAL' }), task({ id: 'high', urgency: 'HIGH' })],
    });

    const top = topOfMatrix(data);
    expect(top.find((t) => t.id === 'crit')?.critical).toBe(true);
    expect(top.find((t) => t.id === 'high')?.critical).toBe(false);
  });

  it('carries the deadline through, null when the task has none', () => {
    const data = quadrants({
      urgentImportant: [
        task({ id: 'dated', deadline: '2026-09-14' }),
        task({ id: 'undated', deadline: null }),
      ],
    });

    const top = topOfMatrix(data);
    expect(top.find((t) => t.id === 'dated')?.deadline).toBe('2026-09-14');
    expect(top.find((t) => t.id === 'undated')?.deadline).toBeNull();
  });
});

describe('matrixBacklog', () => {
  it('counts nothing before the matrix has loaded', () => {
    expect(matrixBacklog(null)).toEqual({ important: 0, urgent: 0, neither: 0 });
  });

  it('counts the three lower quadrants, and never the urgent-and-important one', () => {
    const data = quadrants({
      urgentImportant: [task({ id: 'q1' }), task({ id: 'q1b' })],
      important: [task({ id: 'i1' }), task({ id: 'i2' })],
      urgent: [task({ id: 'u1' })],
      neither: [task({ id: 'n1' }), task({ id: 'n2' }), task({ id: 'n3' })],
    });

    expect(matrixBacklog(data)).toEqual({ important: 2, urgent: 1, neither: 3 });
  });

  it('leaves out a critical task, which is listed above rather than left below', () => {
    // A count that included it would send the reader looking for something
    // that is not down there — it is on the panel already.
    const data = quadrants({
      urgent: [task({ id: 'crit', urgency: 'CRITICAL' }), task({ id: 'plain' })],
    });

    expect(matrixBacklog(data).urgent).toBe(1);
  });

  it('leaves out closed tasks', () => {
    const data = quadrants({
      neither: [task({ id: 'done', status: 'DONE' }), task({ id: 'open' })],
    });

    expect(matrixBacklog(data).neither).toBe(1);
  });

  it('counts a recurring series once, like the list above it', () => {
    const data = quadrants({
      important: [
        task({ id: 'occ-1', isRecurring: true, recurrenceId: 'r1', occurrenceDate: '2026-09-11' }),
        task({ id: 'occ-2', isRecurring: true, recurrenceId: 'r1', occurrenceDate: '2026-09-18' }),
        task({ id: 'occ-3', isRecurring: true, recurrenceId: 'r1', occurrenceDate: '2026-09-25' }),
      ],
    });

    expect(matrixBacklog(data).important).toBe(1);
  });
});
