import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { MemoryRouter, Routes, Route, useNavigate } from 'react-router-dom';

vi.mock('@/hooks/use-session', () => ({
  useSession: () => ({ session: { authenticated: false, account: null }, signOut: vi.fn() }),
}));
vi.mock('@/components/search/HeaderSearchBar', () => ({ HeaderSearchBar: () => null }));

import { PageLayout } from './PageLayout';

// jsdom applies no stylesheet, so the source text is the only way to assert
// the contracts that live in CSS — the same technique the HUD's own tests use.
const SHELL_CSS = readFileSync(resolve(__dirname, '../../styles/app-shell.css'), 'utf8');

function Jump({ to }: { readonly to: string }) {
  const navigate = useNavigate();
  return (
    <button type="button" onClick={() => navigate(to)}>
      go
    </button>
  );
}

function renderShell(initial = '/dashboard') {
  return render(
    <MemoryRouter initialEntries={[initial]}>
      <Routes>
        <Route
          path="*"
          element={
            <PageLayout title="Dashboard">
              <section>content</section>
              <Jump to="/timesheet" />
            </PageLayout>
          }
        />
      </Routes>
    </MemoryRouter>,
  );
}

describe('PageLayout', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('plays the arrival without holding the page back', () => {
    // Same rule as the HUD's opening: the content is mounted and readable from
    // the first frame. The animation brings it in; it never gates it.
    const { container } = renderShell();

    expect(container.querySelector('.app-page')).toBeInTheDocument();
    expect(screen.getByText('content')).toBeInTheDocument();
    expect(screen.getByTestId('app-sweep')).toBeInTheDocument();
  });

  it('takes the sweep away once it has passed', () => {
    renderShell();
    act(() => void vi.advanceTimersByTime(1500));
    expect(screen.queryByTestId('app-sweep')).not.toBeInTheDocument();
  });

  it('replays on navigation, by replacing the wrapper rather than updating it', () => {
    // The entrance restarts because the keyed wrapper is a genuinely NEW
    // element on the new route — no attribute dance needed, unlike the HUD,
    // whose grid never unmounts. A stable key would silently kill the effect.
    const { container } = renderShell();
    const first = container.querySelector('.app-page');
    act(() => void vi.advanceTimersByTime(1500));
    expect(screen.queryByTestId('app-sweep')).not.toBeInTheDocument();

    act(() => void screen.getByText('go').click());

    expect(container.querySelector('.app-page')).not.toBe(first);
    expect(screen.getByTestId('app-sweep')).toBeInTheDocument();
  });
});

describe('the shell stylesheet', () => {
  it('paints no background on body, which is what keeps the overlay see-through', () => {
    // The Tauri window is transparent. A solid `body` would make the whole HUD
    // opaque and undo the overlay entirely, so the ground is painted by the
    // things that want one instead.
    const bodyRule = SHELL_CSS.match(/\nbody \{[^}]*\}/)?.[0] ?? '';
    expect(bodyRule).not.toMatch(/background/);
    expect(SHELL_CSS).toMatch(/\.app-shell \{[^}]*background:\s*var\(--cn-bg\)/);
    expect(SHELL_CSS).toMatch(/\.app-ground \{[^}]*background:\s*var\(--cn-bg\)/);
  });

  it('gives every native form control a dark ground', () => {
    // A class remap only reaches elements carrying a class; an `<input>` with
    // no background class kept the user agent's white.
    expect(SHELL_CSS).toMatch(/input,\s*\ntextarea,\s*\nselect \{[^}]*background-color:/);
    expect(SHELL_CSS).toMatch(/color-scheme:\s*dark/);
  });

  it('cancels the arrival for anyone who asked for less motion', () => {
    const reduced = SHELL_CSS.match(/@media \(prefers-reduced-motion: reduce\) \{[\s\S]*?\n\}/)?.[0] ?? '';
    expect(reduced).toMatch(/animation:\s*none/);
    expect(reduced).toMatch(/\.app-sweep \{\s*display:\s*none/);
  });

  it('reads the desktop palette, never a literal colour', () => {
    // One exception, deliberate: the rail and header sit a step *under* the
    // page ground, which needs mixing toward black — there is no token darker
    // than --cn-bg to mix with.
    const hexes = SHELL_CSS.match(/#[0-9a-fA-F]{3,8}\b/g) ?? [];
    expect(hexes.every((h) => h === '#000')).toBe(true);
    expect(SHELL_CSS).toMatch(/var\(--cn-teal\)/);
  });
});
