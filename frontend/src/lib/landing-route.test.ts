import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: vi.fn(),
}));

import { isTauri } from '@tauri-apps/api/core';
import { landingRoute } from './landing-route';

describe('landingRoute', () => {
  it('lands on /hud when running inside the Tauri window', () => {
    vi.mocked(isTauri).mockReturnValue(true);
    expect(landingRoute()).toBe('/hud');
  });

  it('lands on /dashboard when running in the browser', () => {
    vi.mocked(isTauri).mockReturnValue(false);
    expect(landingRoute()).toBe('/dashboard');
  });
});
