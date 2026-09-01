import type { ReactNode } from 'react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { Provider, type Client } from 'urql';
import { useActiveBreak, type ActiveBreak } from './useActiveBreak';

const SESSION: ActiveBreak = {
  eventId: 'evt-1',
  kind: 'VISUAL',
  label: 'Pause visuelle',
  body: 'Regarde au loin 20 s, relâche les épaules',
  startedAt: '2026-09-01T10:00:00.000Z',
  endsAt: '2026-09-01T10:00:30.000Z',
};

/** What the next `client.query()` resolves with. Mutated per test. */
let answer: { data?: { activeBreak: ActiveBreak | null }; error?: unknown } = {
  data: { activeBreak: SESSION },
};

const query = vi.fn(() => ({ toPromise: () => Promise.resolve(answer) }));

/** A stand-in for the real client. `executeQuery` is what the hook's own
 *  lookup keys on to tell a client apart from urql's empty context default,
 *  so the stub has to carry one. */
const client = { executeQuery: vi.fn(), query } as unknown as Client;

const wrapper = ({ children }: { children: ReactNode }) => (
  <Provider value={client}>{children}</Provider>
);

/** jsdom reports `visible` and offers no way to toggle it, so the property is
 *  redefined and the event fired by hand — the pair `useSurfaceVisibility`
 *  listens for outside Tauri. */
function setVisibility(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state });
  document.dispatchEvent(new Event('visibilitychange'));
}

/** Advances the fake clock and lets the query promises settle. */
async function flush(ms = 0) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

describe('useActiveBreak', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    query.mockClear();
    answer = { data: { activeBreak: SESSION } };
    setVisibility('visible');
  });

  afterEach(() => {
    vi.useRealTimers();
    setVisibility('visible');
  });

  it('asks for the running session as soon as it mounts', async () => {
    const { result } = renderHook(() => useActiveBreak(), { wrapper });
    await flush();

    expect(query).toHaveBeenCalledTimes(1);
    expect(result.current).toEqual(SESSION);
  });

  it('reports no break when the API says there is none', async () => {
    answer = { data: { activeBreak: null } };
    const { result } = renderHook(() => useActiveBreak(), { wrapper });
    await flush();

    expect(result.current).toBeNull();
  });

  it('keeps asking every two seconds while the surface is visible', async () => {
    renderHook(() => useActiveBreak(), { wrapper });
    await flush();
    expect(query).toHaveBeenCalledTimes(1);

    await flush(2000);
    expect(query).toHaveBeenCalledTimes(2);

    await flush(2000);
    expect(query).toHaveBeenCalledTimes(3);
  });

  it('stops asking while the surface is hidden', async () => {
    renderHook(() => useActiveBreak(), { wrapper });
    await flush();
    expect(query).toHaveBeenCalledTimes(1);

    act(() => setVisibility('hidden'));
    await flush(10_000);

    // The whole point: no polling all day long behind a hidden workspace.
    expect(query).toHaveBeenCalledTimes(1);
  });

  it('asks again the moment the surface comes back, without waiting for a tick', async () => {
    renderHook(() => useActiveBreak(), { wrapper });
    await flush();
    act(() => setVisibility('hidden'));
    await flush(10_000);

    act(() => setVisibility('visible'));
    await flush();

    expect(query).toHaveBeenCalledTimes(2);
  });

  it('forgets the session while hidden, so the overlay never reopens onto a stale break', async () => {
    const { result } = renderHook(() => useActiveBreak(), { wrapper });
    await flush();
    expect(result.current).toEqual(SESSION);

    act(() => setVisibility('hidden'));

    // The break may well have ended behind the curtain; the hook has no way to
    // know until it asks again, and showing the old one for a frame would be
    // a lie the user would see.
    expect(result.current).toBeNull();
  });

  it('holds the running session through a failed poll', async () => {
    const { result } = renderHook(() => useActiveBreak(), { wrapper });
    await flush();
    expect(result.current).toEqual(SESSION);

    answer = { error: new Error('offline') };
    await flush(2000);

    // A blip on the wire must not drop the user out of a break that is running.
    expect(result.current).toEqual(SESSION);
  });

  it('reports no break when the overlay is mounted without a data layer', async () => {
    // `HudPage` calls this above its boot gate — earlier than anything else in
    // the overlay queries — so it is the one hook that can find itself outside
    // urql's Provider. That has to read as "no break", not as a crash that
    // takes the whole surface down.
    const { result } = renderHook(() => useActiveBreak());
    await flush();

    expect(result.current).toBeNull();
    expect(query).not.toHaveBeenCalled();
  });
});
