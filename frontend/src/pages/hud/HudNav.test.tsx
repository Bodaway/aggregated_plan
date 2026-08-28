import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { describe, it, expect } from 'vitest';
import { HudNav } from './HudNav';

const at = (path: string) =>
  render(<MemoryRouter initialEntries={[path]}><HudNav /></MemoryRouter>);

describe('HudNav', () => {
  it('lists every destination of the application', () => {
    at('/hud');
    expect(screen.getAllByRole('link').length).toBe(12); // 11 real routes + the HUD itself
  });

  it('marks the current view as the one that is lit', () => {
    at('/hud');
    expect(screen.getByRole('link', { name: /hud/i })).toHaveAttribute('aria-current', 'page');
  });

  it('jumps to a destination when its digit is pressed', async () => {
    at('/hud');
    await userEvent.keyboard('1');
    expect(screen.getByRole('link', { name: /dashboard/i })).toHaveAttribute('aria-current', 'page');
  });

  it('ignores the digit when a text field has focus', async () => {
    at('/hud');
    const input = document.createElement('input');
    document.body.appendChild(input);
    input.focus();
    await userEvent.keyboard('1');
    expect(screen.getByRole('link', { name: /hud/i })).toHaveAttribute('aria-current', 'page');
    input.remove();
  });
});
