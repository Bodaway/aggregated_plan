import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';

// The only thing this screen sends is `endBreak`, and the mutation's own
// wiring is the API's business — mocked here, house style, so the test stays
// about the countdown and the one button.
const endBreak = vi.fn();
vi.mock('urql', () => ({
  useMutation: () => [{ fetching: false }, endBreak],
}));

import { BreakScreen } from './BreakScreen';
import type { ActiveBreak } from './useActiveBreak';

const START = '2026-09-01T10:00:00.000Z';
const END = '2026-09-01T10:05:00.000Z';
const T0 = Date.parse(START);

const SESSION: ActiveBreak = {
  eventId: 'evt-1',
  kind: 'LONG',
  label: 'Pause franche',
  body: "Lève-toi, marche, bois un verre d'eau",
  startedAt: START,
  endsAt: END,
};

const renderScreen = (session: ActiveBreak = SESSION) => render(<BreakScreen session={session} />);

/** Moves the fake clock forward by `ms`, tick by tick. */
const tick = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

const remaining = () => screen.getByRole('timer').textContent;

/** The fraction of the ring that has been spent, read off the two attributes
 *  that draw it — the relationship, not the radius, is what is designed. */
function elapsedFraction(): number {
  const ring = screen.getByTestId('break-ring');
  const total = Number(ring.getAttribute('stroke-dasharray'));
  const offset = Number(ring.getAttribute('stroke-dashoffset'));
  return offset / total;
}

describe('BreakScreen', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(T0));
    endBreak.mockReset();
    endBreak.mockResolvedValue({ data: { endBreak: true }, error: undefined });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows the rule in its own words', () => {
    renderScreen();
    expect(screen.getByText('Pause franche')).toBeInTheDocument();
    expect(screen.getByText("Lève-toi, marche, bois un verre d'eau")).toBeInTheDocument();
  });

  it('reads the remaining time off endsAt', () => {
    renderScreen();
    expect(remaining()).toBe('5:00');
  });

  it('counts down as the clock runs', () => {
    renderScreen();
    tick(90_000);
    expect(remaining()).toBe('3:30');
  });

  it('pads the seconds under a minute', () => {
    renderScreen();
    tick(241_000);
    expect(remaining()).toBe('0:59');
  });

  it('recomputes from the deadline after the webview is starved', () => {
    // The bug this screen must not have: a decremented counter drifts the
    // moment the webview is throttled and its ticks are dropped. Here four
    // minutes of wall clock pass with a single tick delivered, and the display
    // has to reflect the clock, not the number of ticks it received.
    renderScreen();
    act(() => {
      vi.setSystemTime(new Date(T0 + 239_000));
      vi.advanceTimersByTime(1000);
    });
    expect(remaining()).toBe('1:00');
  });

  it('empties the ring in step with the time spent', () => {
    renderScreen();
    expect(elapsedFraction()).toBeCloseTo(0, 3);

    tick(150_000);
    expect(elapsedFraction()).toBeCloseTo(0.5, 3);

    tick(75_000);
    expect(elapsedFraction()).toBeCloseTo(0.75, 3);
  });

  it('announces the remaining time', () => {
    renderScreen();
    // A live region the assistive stack can read on demand — the countdown is
    // the one number on this screen.
    expect(screen.getByRole('timer')).toHaveTextContent('5:00');
  });

  it('ends the break with its own event id', async () => {
    renderScreen();
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /j'y retourne/i }));
    });
    expect(endBreak).toHaveBeenCalledWith({ eventId: 'evt-1' });
  });

  it('reaches the button from the keyboard', () => {
    renderScreen();
    const button = screen.getByRole('button', { name: /j'y retourne/i });
    button.focus();
    expect(button).toHaveFocus();
  });

  it('says nothing when the countdown won the race', async () => {
    // `endBreak` answers false when the tick closed the row first, in the very
    // second the button was pressed. That is the normal outcome of a race, not
    // a failure anyone needs to be told about.
    endBreak.mockResolvedValue({ data: { endBreak: false }, error: undefined });
    renderScreen();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /j'y retourne/i }));
    });

    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.getByRole('timer')).toBeInTheDocument();
  });

  it('settles into a finished state at zero, and stays there', () => {
    // Nothing to do at zero: the backend closes the row and hides the surface.
    // This is only what the screen looks like if the overlay lingers a second.
    renderScreen();
    tick(300_000);

    expect(remaining()).toBe('0:00');
    expect(elapsedFraction()).toBeCloseTo(1, 3);
    expect(screen.getByTestId('break-screen')).toHaveAttribute('data-state', 'done');

    tick(60_000);
    expect(remaining()).toBe('0:00');
    expect(elapsedFraction()).toBeCloseTo(1, 3);
  });
});
