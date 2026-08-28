import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen, act, cleanup } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Read directly, same technique FocusBlock.test.tsx uses — jsdom does not
// apply this stylesheet, so a rendered element's computed style can't tell
// us whether the degraded (non-Tauri) state is actually styled as
// deliberate, or whether a leaked --hud-ink-low text colour has crept in.
const HUD_CSS = readFileSync(resolve(__dirname, '../hud.css'), 'utf8');

// The IPC boundary this block reads from — mocked at the module boundary so
// the tests exercise the block's own degrade logic (Tauri vs. browser,
// polling gated on surface visibility), not the real Tauri runtime, which
// does not exist under Vitest/jsdom anyway.
vi.mock('@tauri-apps/api/core', () => ({
  isTauri: vi.fn(),
  invoke: vi.fn(),
}));

// Inside Tauri, surface visibility no longer comes from the document — it
// arrives as a `surface-visibility` event the shell emits when the toggle
// script signals it (the webview itself never notices a Hyprland
// special-workspace hide; measured on the real compositor). `vi.hoisted`
// because `vi.mock` is lifted above ordinary declarations.
const surfaceEvents = vi.hoisted(() => ({
  subscribers: new Set<(event: { payload: boolean }) => void>(),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: (_name: string, handler: (event: { payload: boolean }) => void) => {
    surfaceEvents.subscribers.add(handler);
    return Promise.resolve(() => surfaceEvents.subscribers.delete(handler));
  },
}));

import { isTauri, invoke } from '@tauri-apps/api/core';
import { StationBlock, type StationStats } from './StationBlock';

/** Drives the shell's `surface-visibility` event — the only signal that works
 *  inside Tauri, the webview being blind to a Hyprland special-workspace hide.
 *  The surface starts SHOWN there (the toggle script launches the binary and
 *  reveals the workspace in one breath), so a test that wants it hidden has to
 *  say so. */
async function showSurface() {
  await act(async () => {
    surfaceEvents.subscribers.forEach((handler) => handler({ payload: true }));
    await Promise.resolve();
  });
}

async function hideSurface() {
  await act(async () => {
    surfaceEvents.subscribers.forEach((handler) => handler({ payload: false }));
    await Promise.resolve();
  });
}

const SAMPLE_STATS: StationStats = {
  cpuPercent: 18,
  ramUsedBytes: 11_400_000_000,
  ramTotalBytes: 32_000_000_000,
  netRxBytesPerSec: 1_000_000,
  netTxBytesPerSec: 1_100_000,
};

describe('StationBlock', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // A Friday — matches the "vendredi 28 août" this session's own clock
    // reads, so the formatted weekday is easy to eyeball against reality.
    vi.setSystemTime(new Date('2026-08-28T14:52:07'));
    vi.mocked(isTauri).mockReset();
    vi.mocked(invoke).mockReset();
  });

  afterEach(() => {
    // Unmount before touching timers/visibility — same ordering FocusBlock's
    // own tests use, to avoid an act() warning from a timer firing on an
    // already-unmounted component.
    cleanup();
    vi.useRealTimers();
    surfaceEvents.subscribers.clear();
  });

  it('shows the clock and date outside Tauri, with no empty cell and no invented system value', async () => {
    vi.mocked(isTauri).mockReturnValue(false);

    render(<StationBlock />);

    expect(screen.getByTestId('station-block')).toBeInTheDocument();
    // Every sibling block carries its own `.hud-label` header — review
    // finding: Station had shipped without one.
    expect(screen.getByText(/▌ Station/)).toBeInTheDocument();
    expect(screen.getByTestId('station-clock')).toHaveTextContent('14:52');
    expect(screen.getByText(/Friday 28 August/i)).toBeInTheDocument();

    // No IPC command exists outside Tauri — the block must not even try it.
    expect(invoke).not.toHaveBeenCalled();

    // Let any stray microtask/timer settle, then confirm the sys grid never
    // appears — not as three empty cells, not with a fabricated value.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(screen.queryByTestId('station-sys')).not.toBeInTheDocument();

    // Regression guard, same technique as PressureBlock's empty-state test:
    // the degraded state reads as deliberate, not merely absent.
    const unavailableRule = HUD_CSS.match(/\.hud-station__unavailable\s*\{[^}]*\}/)?.[0] ?? '';
    expect(unavailableRule).toMatch(/font-style:\s*italic/);
  });

  it('polls the Tauri IPC command and renders CPU, RAM and network once a sample arrives', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(SAMPLE_STATS);

    render(<StationBlock />);
    await showSurface();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(invoke).toHaveBeenCalledWith('station_stats');
    expect(screen.getByTestId('station-sys')).toBeInTheDocument();
    expect(screen.getByText('18%')).toBeInTheDocument();
    expect(screen.getByText('11.4 G')).toBeInTheDocument();
    expect(screen.getByText('2.1 M/s')).toBeInTheDocument();
    expect(screen.queryByTestId('station-unavailable')).not.toBeInTheDocument();
  });

  it('polls again on the next tick and keeps polling while visible', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(SAMPLE_STATS);

    render(<StationBlock />);
    await showSurface();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    const callsAfterMount = vi.mocked(invoke).mock.calls.length;

    await act(async () => {
      await vi.advanceTimersByTimeAsync(4000);
    });

    expect(vi.mocked(invoke).mock.calls.length).toBeGreaterThan(callsAfterMount);
  });

  it('does not poll system stats while the surface is hidden', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(SAMPLE_STATS);

    render(<StationBlock />);
    await hideSurface();
    vi.mocked(invoke).mockClear();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });

    expect(invoke).not.toHaveBeenCalled();
  });

  it('resumes polling once the surface becomes visible again', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(SAMPLE_STATS);

    render(<StationBlock />);
    await hideSurface();
    vi.mocked(invoke).mockClear();

    await showSurface();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(invoke).toHaveBeenCalledWith('station_stats');
  });

  it('stops polling again when the surface is hidden', async () => {
    // The other half of the signal: SIGUSR2 must actually put the block back
    // to sleep, not merely fail to wake it.
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(SAMPLE_STATS);

    render(<StationBlock />);
    await showSurface();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4000);
    });
    const callsWhileVisible = vi.mocked(invoke).mock.calls.length;
    expect(callsWhileVisible).toBeGreaterThan(0);

    await hideSurface();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });

    expect(vi.mocked(invoke).mock.calls.length).toBe(callsWhileVisible);
  });

  it('ticks the clock forward once a minute passes, gated on surface visibility', async () => {
    vi.mocked(isTauri).mockReturnValue(false);

    render(<StationBlock />);
    expect(screen.getByTestId('station-clock')).toHaveTextContent('14:52');

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(screen.getByTestId('station-clock')).toHaveTextContent('14:53');
  });
});
