import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SelectionToMemory } from './SelectionToMemory';

const onRemember = vi.fn();

const CHIP = /capture selection as memory/i;

const DEFAULT_RECT = { top: 100, left: 50, bottom: 118, right: 300, width: 250, height: 18 };

function stubSelection(
  text: string,
  anchor: Node | null = null,
  rect: Partial<typeof DEFAULT_RECT> = {}
) {
  const node = anchor ?? document.createTextNode(text);
  window.getSelection = () =>
    ({
      toString: () => text,
      isCollapsed: text === '',
      anchorNode: node,
      rangeCount: text === '' ? 0 : 1,
      getRangeAt: () => ({
        getBoundingClientRect: () => ({ ...DEFAULT_RECT, ...rect }),
      }),
    }) as unknown as Selection;
  fireEvent.mouseUp(document);
}

beforeEach(() => {
  onRemember.mockReset();
  stubSelection('');
});

describe('SelectionToMemory', () => {
  it('stays out of the way until something is selected', () => {
    render(<SelectionToMemory onRemember={onRemember} />);

    expect(screen.queryByRole('button', { name: CHIP })).not.toBeInTheDocument();
  });

  it('offers to capture once text is selected', () => {
    render(<SelectionToMemory onRemember={onRemember} />);

    stubSelection('Le compte -ext seul accède au ServiceNow de Pernod.');

    expect(screen.getByRole('button', { name: CHIP })).toBeInTheDocument();
  });

  it('ignores a selection too short to be a memory', () => {
    render(<SelectionToMemory onRemember={onRemember} />);

    stubSelection('ok');

    expect(screen.queryByRole('button', { name: CHIP })).not.toBeInTheDocument();
  });

  it('opens the sheet with the selection split into a title and a body', () => {
    render(<SelectionToMemory onRemember={onRemember} />);
    const first = 'Le compte Witivio est refusé par le ServiceNow de Pernod Ricard.';
    const rest = 'Toute consultation doit donc partir du compte -ext, la politique vient de Pernod.';

    stubSelection(`${first} ${rest}`);
    fireEvent.click(screen.getByRole('button', { name: CHIP }));

    expect(screen.getByLabelText('Title')).toHaveValue(first);
    expect(screen.getByLabelText('Why')).toHaveValue(rest);
  });

  it('records the memory the sheet submits', () => {
    render(<SelectionToMemory onRemember={onRemember} />);

    stubSelection('Le compte -ext seul accède au ServiceNow de Pernod.');
    fireEvent.click(screen.getByRole('button', { name: CHIP }));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(onRemember).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: 'FACT',
        title: 'Le compte -ext seul accède au ServiceNow de Pernod.',
        confirmed: false,
      })
    );
  });

  it('attaches the task whose card the selection came from', () => {
    render(
      <>
        <div data-task-id="task-42">
          <p>Un constat noté sur la carte de la tâche.</p>
        </div>
        <SelectionToMemory onRemember={onRemember} />
      </>
    );
    const paragraph = screen.getByText('Un constat noté sur la carte de la tâche.');

    stubSelection('Un constat noté sur la carte de la tâche.', paragraph);
    fireEvent.click(screen.getByRole('button', { name: CHIP }));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(onRemember).toHaveBeenCalledWith(expect.objectContaining({ taskId: 'task-42' }));
  });
  it('opens the sheet from the keyboard once a selection is captured', () => {
    render(<SelectionToMemory onRemember={onRemember} />);

    stubSelection('Le compte -ext seul accède au ServiceNow de Pernod.');
    fireEvent.keyDown(document, { key: 'm', ctrlKey: true });

    expect(screen.getByLabelText('Title')).toBeInTheDocument();
  });

  it('places the chip just under the selection', () => {
    render(<SelectionToMemory onRemember={onRemember} />);

    stubSelection('Le compte -ext seul accède au ServiceNow de Pernod.');

    const chip = screen.getByRole('button', { name: CHIP });
    expect(chip.style.top).toBe('126px');
    expect(chip.style.left).toBe('50px');
  });

  it('ignores a selection that has no on-screen geometry', () => {
    render(<SelectionToMemory onRemember={onRemember} />);

    // A range whose nodes React has just replaced still stringifies, but its
    // rect collapses to 0×0 at the origin — which used to park the chip in the
    // top-left corner of the screen.
    stubSelection('Le compte -ext seul accède au ServiceNow de Pernod.', null, {
      top: 0,
      left: 0,
      bottom: 0,
      right: 0,
      width: 0,
      height: 0,
    });

    expect(screen.queryByRole('button', { name: CHIP })).not.toBeInTheDocument();
  });

  it('keeps the chip inside the viewport when the selection hugs an edge', () => {
    render(<SelectionToMemory onRemember={onRemember} />);

    stubSelection('Le compte -ext seul accède au ServiceNow de Pernod.', null, {
      left: window.innerWidth - 10,
      right: window.innerWidth,
      top: window.innerHeight - 12,
      bottom: window.innerHeight - 2,
    });

    const chip = screen.getByRole('button', { name: CHIP });
    expect(parseFloat(chip.style.left)).toBeLessThanOrEqual(window.innerWidth - 100);
    expect(parseFloat(chip.style.top)).toBeLessThan(window.innerHeight - 12);
  });

  it('drops the capture when the selection goes away without a click', () => {
    render(<SelectionToMemory onRemember={onRemember} />);
    stubSelection('Le compte -ext seul accède au ServiceNow de Pernod.');
    expect(screen.getByRole('button', { name: CHIP })).toBeInTheDocument();

    window.getSelection = () =>
      ({ toString: () => '', isCollapsed: true, anchorNode: null, rangeCount: 0 }) as unknown as Selection;
    fireEvent(document, new Event('selectionchange'));

    expect(screen.queryByRole('button', { name: CHIP })).not.toBeInTheDocument();
  });

  it('ignores a selection that has scrolled out of the viewport', () => {
    render(<SelectionToMemory onRemember={onRemember} />);

    stubSelection('Le compte -ext seul accède au ServiceNow de Pernod.', null, {
      top: window.innerHeight + 200,
      bottom: window.innerHeight + 218,
    });

    expect(screen.queryByRole('button', { name: CHIP })).not.toBeInTheDocument();
  });
});
