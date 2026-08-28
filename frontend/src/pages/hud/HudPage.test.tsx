import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';

// The grid now also hosts FocusBlock, which reads real data hooks (urql
// underneath) — mocked here so this page-level test stays about the boot
// sequence / grid handoff, not about GraphQL wiring (FocusBlock.test.tsx
// owns that).
vi.mock('@/hooks/use-activity', () => ({ useActivity: () => ({ currentActivity: null }) }));
vi.mock('@/hooks/use-timesheet', () => ({ useTimesheet: () => ({ day: null }) }));
vi.mock('@/hooks/use-dashboard', () => ({ useDashboard: () => ({ data: null }) }));
vi.mock('@/hooks/use-break-rules', () => ({ useNextBreakDue: () => ({ nextBreakDue: null, refetch: vi.fn() }) }));

import { HudPage } from './HudPage';

// The grid now hosts HudNav, which reads router context (useLocation /
// useNavigate) — so every render needs a Router, same as it gets from
// BrowserRouter in the real app.
const renderHudPage = () => render(<HudPage />, { wrapper: MemoryRouter });

describe('HudPage', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows the boot sequence first', () => {
    renderHudPage();
    expect(screen.getByTestId('boot-sequence')).toBeInTheDocument();
    expect(screen.queryByTestId('hud-grid')).not.toBeInTheDocument();
  });

  it('gives way to the grid after the sequence', () => {
    renderHudPage();
    act(() => void vi.advanceTimersByTime(1600));
    expect(screen.queryByTestId('boot-sequence')).not.toBeInTheDocument();
    expect(screen.getByTestId('hud-grid')).toBeInTheDocument();
  });

  it('paints a transparent background, the window being transparent', () => {
    const { container } = renderHudPage();
    const root = container.firstElementChild as HTMLElement;
    expect(root.className).toContain('bg-transparent');
  });
});
