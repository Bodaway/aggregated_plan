import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';

import { TimesheetTimeline, UNATTRIBUTED_LABEL } from './TimesheetTimeline';
import type { AttributedBlock, ProjectOption, UnresolvedSignal } from '@/hooks/use-timesheet';

// NOTE: bare local NaiveDateTime (no `Z`/offset) — exactly the wire format the backend
// emits — so new Date(iso).getHours() is deterministic in any test-runner timezone.
const blocks: AttributedBlock[] = [
  { startTime: '2026-06-08T08:00:00', endTime: '2026-06-08T10:00:00', gryzzlyProjectId: 'p1', kind: 'WORK', hours: 2, sourceRefs: [], originLabel: null },
  { startTime: '2026-06-08T09:00:00', endTime: '2026-06-08T10:00:00', gryzzlyProjectId: null, kind: 'MEETING', hours: 1, sourceRefs: [], originLabel: null },
  { startTime: '2026-06-08T14:00:00', endTime: '2026-06-08T16:00:00', gryzzlyProjectId: 'p1', kind: 'WORK', hours: 2, sourceRefs: [], originLabel: null },
];

// A raw id the UI must never show once a catalog entry exists for it.
const PROJECT_ID = 'aaaaaaaa-1111-2222-3333-444444444444';
const projects: ProjectOption[] = [{ id: PROJECT_ID, label: 'Refonte portail — ACME' }];

// The name of the task a block came from, rendered under its project name.
const TASK_NAME = 'Corriger le parseur de signaux';

// 08:00–10:00 is half the morning window, well past both the ~10% cut-off below which a
// bar renders no label at all and the wider cut-off for its second line.
function morningBlock(over: Partial<AttributedBlock> = {}): AttributedBlock {
  return {
    startTime: '2026-06-08T08:00:00',
    endTime: '2026-06-08T10:00:00',
    gryzzlyProjectId: null,
    kind: 'WORK',
    hours: 2,
    sourceRefs: [],
    originLabel: null,
    ...over,
  };
}

/** The `text-[Npx]` size a label line renders at. Comparing classes keeps the assertion
 *  on what the component decides, not on jsdom's (absent) layout. */
function fontSizePx(el: Element): number {
  const match = el.className.match(/text-\[(\d+)px\]/);
  if (!match) throw new Error(`no text-[Npx] class on <${el.tagName}> "${el.textContent}"`);
  return Number(match[1]);
}

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

  it('labels a WORK block with no gryzzly project as unattributed, never as "??"', () => {
    const { container } = render(<TimesheetTimeline blocks={[morningBlock()]} />);
    expect(screen.getByText(UNATTRIBUTED_LABEL)).toBeInTheDocument();
    expect(screen.queryByText('??')).not.toBeInTheDocument();
    expect(container.textContent).not.toContain('??');
  });

  it('labels a resolved WORK block with the catalog name, not the raw project id', () => {
    render(<TimesheetTimeline blocks={[morningBlock({ gryzzlyProjectId: PROJECT_ID })]} projects={projects} />);
    expect(screen.getByText('Refonte portail — ACME')).toBeInTheDocument();
    expect(screen.queryByText(PROJECT_ID)).not.toBeInTheDocument();
  });

  it('falls back to the raw project id when the projects catalog is empty', () => {
    render(<TimesheetTimeline blocks={[morningBlock({ gryzzlyProjectId: PROJECT_ID })]} projects={[]} />);
    expect(screen.getByText(PROJECT_ID)).toBeInTheDocument();
  });

  it('falls back to the raw project id when no projects prop is passed at all', () => {
    render(<TimesheetTimeline blocks={[morningBlock({ gryzzlyProjectId: PROJECT_ID })]} />);
    expect(screen.getByText(PROJECT_ID)).toBeInTheDocument();
  });

  it('appends the note of the matching unresolved signal to an unattributed block tooltip', () => {
    const unresolved: UnresolvedSignal[] = [
      { sourceRef: 'wl:11111111-1111-1111-1111-111111111111', label: 'Refactor du parseur de signaux', at: '2026-06-08T08:30:00' },
      { sourceRef: 'wl:99999999-9999-9999-9999-999999999999', label: 'Signal d’un autre bloc', at: '2026-06-08T15:00:00' },
    ];
    const block = morningBlock({ sourceRefs: ['wl:11111111-1111-1111-1111-111111111111'] });
    const { container } = render(<TimesheetTimeline blocks={[block]} unresolved={unresolved} />);

    const bar = container.querySelector('[data-block]');
    const title = bar?.getAttribute('title') ?? '';
    expect(title).toContain(UNATTRIBUTED_LABEL);
    expect(title).toContain('Refactor du parseur de signaux');
    // Only the signals sharing a sourceRef with this block may leak into its tooltip.
    expect(title).not.toContain('Signal d’un autre bloc');
  });

  it('renders the task name under the project name', () => {
    render(
      <TimesheetTimeline
        blocks={[morningBlock({ gryzzlyProjectId: PROJECT_ID, originLabel: TASK_NAME })]}
        projects={projects}
      />,
    );
    expect(screen.getByText('Refonte portail — ACME')).toBeInTheDocument();
    expect(screen.getByText(TASK_NAME)).toBeInTheDocument();
  });

  it('renders the task name in a smaller class than the project name', () => {
    render(
      <TimesheetTimeline
        blocks={[morningBlock({ gryzzlyProjectId: PROJECT_ID, originLabel: TASK_NAME })]}
        projects={projects}
      />,
    );
    const project = fontSizePx(screen.getByText('Refonte portail — ACME'));
    const task = fontSizePx(screen.getByText(TASK_NAME));
    expect(task).toBeLessThan(project);
  });

  it('names both the project and the task in the tooltip', () => {
    const { container } = render(
      <TimesheetTimeline
        blocks={[morningBlock({ gryzzlyProjectId: PROJECT_ID, originLabel: TASK_NAME })]}
        projects={projects}
      />,
    );
    const title = container.querySelector('[data-block]')?.getAttribute('title') ?? '';
    expect(title).toContain('Refonte portail — ACME');
    expect(title).toContain(TASK_NAME);
  });

  it('renders only the project line when the block has no task name', () => {
    render(
      <TimesheetTimeline
        blocks={[morningBlock({ gryzzlyProjectId: PROJECT_ID, originLabel: null })]}
        projects={projects}
      />,
    );
    const bar = screen.getByText('Refonte portail — ACME').closest('[data-block]');
    expect(bar?.childElementCount).toBe(1);
  });

  it('hides the task name on a block too narrow for a second line but wide enough for the first', () => {
    // 30 min of the 240-min morning window = 12.5%: past the ~10% label cut-off, short of
    // the wider cut-off a second line needs.
    render(
      <TimesheetTimeline
        blocks={[
          morningBlock({
            startTime: '2026-06-08T08:00:00',
            endTime: '2026-06-08T08:30:00',
            gryzzlyProjectId: PROJECT_ID,
            originLabel: TASK_NAME,
          }),
        ]}
        projects={projects}
      />,
    );
    expect(screen.getByText('Refonte portail — ACME')).toBeInTheDocument();
    expect(screen.queryByText(TASK_NAME)).not.toBeInTheDocument();
  });

  it('renders neither line on a narrow block', () => {
    // 12 min of the morning window = 5%, under the label cut-off: a clipped half-character
    // is worse than no label.
    const { container } = render(
      <TimesheetTimeline
        blocks={[
          morningBlock({
            startTime: '2026-06-08T08:00:00',
            endTime: '2026-06-08T08:12:00',
            gryzzlyProjectId: PROJECT_ID,
            originLabel: TASK_NAME,
          }),
        ]}
        projects={projects}
      />,
    );
    expect(screen.queryByText('Refonte portail — ACME')).not.toBeInTheDocument();
    expect(screen.queryByText(TASK_NAME)).not.toBeInTheDocument();
    expect(container.querySelector('[data-block]')?.childElementCount).toBe(0);
  });

  it('labels an OUT_OF_OFFICE block as an absence rather than unattributed', () => {
    render(<TimesheetTimeline blocks={[morningBlock({ kind: 'OUT_OF_OFFICE' })]} />);
    expect(screen.getByText('absence')).toBeInTheDocument();
    expect(screen.queryByText(UNATTRIBUTED_LABEL)).not.toBeInTheDocument();
  });
});
