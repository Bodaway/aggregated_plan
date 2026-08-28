import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { Sidebar } from './Sidebar';

const EXPECTED_ROUTES = [
  '/dashboard',
  '/triage',
  '/priority',
  '/workload',
  '/activity',
  '/timesheet',
  '/worklog',
  '/memory',
  '/dedup',
  '/alerts',
  '/settings',
  '/hud',
];

describe('Sidebar', () => {
  it('links to the memory tab', () => {
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    );

    expect(screen.getByRole('link', { name: 'Memory' })).toHaveAttribute('href', '/memory');
  });

  it('links to every application route, including the HUD escape hatch', () => {
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    );

    const hrefs = screen.getAllByRole('link').map(link => link.getAttribute('href'));

    expect(hrefs).toEqual(EXPECTED_ROUTES);
  });
});
