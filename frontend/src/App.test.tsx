import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';
import { App } from './App';

vi.mock('@/lib/search/SearchProvider', () => ({
  SearchProvider: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

vi.mock('@/lib/landing-route', () => ({
  landingRoute: vi.fn(() => '/hud'),
}));

describe('App', () => {
  afterEach(() => {
    window.history.pushState({}, '', '/');
  });

  it('sends an unmatched path to landingRoute()', () => {
    window.history.pushState({}, '', '/this-route-does-not-exist');
    render(<App />);
    expect(screen.getByTestId('boot-sequence')).toBeInTheDocument();
  });
});
