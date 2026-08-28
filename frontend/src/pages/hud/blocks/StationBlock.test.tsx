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

import { isTauri, invoke } from '@tauri-apps/api/core';
import { StationBlock, type StationStats } from './StationBlock';

function setVisibility(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => state,
  });
  document.dispatchEvent(new Event('visibilitychange'));
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
    setVisibility('visible');
  });

  it('shows the clock and date outside Tauri, with no empty cell and no invented system value', async () => {
    vi.mocked(isTauri).mockReturnValue(false);

    render(<StationBlock />);

    expect(screen.getByTestId('station-block')).toBeInTheDocument();
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
    setVisibility('hidden');
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(SAMPLE_STATS);

    render(<StationBlock />);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });

    expect(invoke).not.toHaveBeenCalled();
    expect(screen.queryByTestId('station-sys')).not.toBeInTheDocument();
  });

  it('resumes polling once the surface becomes visible again', async () => {
    setVisibility('hidden');
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(SAMPLE_STATS);

    render(<StationBlock />);

    await act(async () => {
      setVisibility('visible');
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(invoke).toHaveBeenCalledWith('station_stats');
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
