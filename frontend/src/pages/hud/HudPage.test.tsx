import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { HudPage } from './HudPage';

describe('HudPage', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows the boot sequence first', () => {
    render(<HudPage />);
    expect(screen.getByTestId('boot-sequence')).toBeInTheDocument();
    expect(screen.queryByTestId('hud-grid')).not.toBeInTheDocument();
  });

  it('gives way to the grid after the sequence', () => {
    render(<HudPage />);
    act(() => void vi.advanceTimersByTime(1600));
    expect(screen.queryByTestId('boot-sequence')).not.toBeInTheDocument();
    expect(screen.getByTestId('hud-grid')).toBeInTheDocument();
  });

  it('paints a transparent background, the window being transparent', () => {
    const { container } = render(<HudPage />);
    const root = container.firstElementChild as HTMLElement;
    expect(root.className).toContain('bg-transparent');
  });
});
