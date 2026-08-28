import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { MemoryRouter, Routes, Route, useLocation } from 'react-router-dom';

vi.mock('@tauri-apps/api/core', () => ({ isTauri: vi.fn() }));

// The surface signal the Tauri shell emits. Hoisted because `vi.mock` is
// lifted above ordinary declarations.
const surfaceEvents = vi.hoisted(() => ({
  subscribers: new Set<(event: { payload: boolean }) => void>(),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: (_name: string, handler: (event: { payload: boolean }) => void) => {
    surfaceEvents.subscribers.add(handler);
    return Promise.resolve(() => surfaceEvents.subscribers.delete(handler));
  },
}));

import { isTauri } from '@tauri-apps/api/core';
import { ReturnToHudOnOpen } from './ReturnToHudOnOpen';

function Where() {
  return <span data-testid="where">{useLocation().pathname}</span>;
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <ReturnToHudOnOpen />
      <Routes>
        <Route path="*" element={<Where />} />
      </Routes>
    </MemoryRouter>,
  );
}

async function emit(shown: boolean) {
  await act(async () => {
    surfaceEvents.subscribers.forEach((handler) => handler({ payload: shown }));
    await Promise.resolve();
  });
}

/** Lets the `listen()` promise settle so the subscription actually exists. */
async function settle() {
  await act(async () => {
    await Promise.resolve();
  });
}

/** jsdom has no setter for `visibilityState`, so the property is redefined and
 *  the event fired by hand — the pair the browser path actually listens for. */
function setDocumentVisibility(state: 'visible' | 'hidden') {
  Object.defineProperty(document, 'visibilityState', { value: state, configurable: true });
  act(() => void document.dispatchEvent(new Event('visibilitychange')));
}

const at = () => screen.getByTestId('where').textContent;

describe('ReturnToHudOnOpen', () => {
  beforeEach(() => vi.mocked(isTauri).mockReset());
  afterEach(() => {
    surfaceEvents.subscribers.clear();
    setDocumentVisibility('visible');
  });

  it('brings a reopened overlay back to the HUD from wherever it was left', async () => {
    // The reported behaviour: close while on Timesheet, reopen, and you were
    // dropped straight back onto Timesheet — no HUD, and no boot sequence
    // either, since that lives in a page that was not mounted.
    vi.mocked(isTauri).mockReturnValue(true);
    renderAt('/timesheet');
    await settle();
    expect(at()).toBe('/timesheet');

    await emit(false);
    await emit(true);

    expect(at()).toBe('/hud');
  });

  it('leaves the route alone while the overlay merely stays open', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    renderAt('/timesheet');
    await settle();

    // A repeated "shown" with no intervening hide is not an opening.
    await emit(true);

    expect(at()).toBe('/timesheet');
  });

  it('does not treat its own mount as an opening', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    renderAt('/settings');
    await settle();

    expect(at()).toBe('/settings');
  });

  it('never navigates in the browser, where the signal means something else', async () => {
    // At :3000 the visibility signal is the document's own, which flips on
    // every workspace switch and tab change. Acting on it there would yank
    // the page away from whatever someone was reading.
    //
    // Driven through the DOCUMENT, not the Tauri event: outside Tauri the
    // hook never subscribes to the event at all, so emitting one proves
    // nothing. Written that way first, this test passed with the `isTauri`
    // guard deleted — it could not fail, which is no test at all.
    vi.mocked(isTauri).mockReturnValue(false);
    renderAt('/timesheet');
    await settle();

    setDocumentVisibility('hidden');
    setDocumentVisibility('visible');

    expect(at()).toBe('/timesheet');
  });
});
