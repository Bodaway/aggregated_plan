import { describe, expect, it } from '@jest/globals';
import { getServerConfig } from './server-config';

describe('getServerConfig', () => {
  it('returns defaults when env is empty', () => {
    const config = getServerConfig({});

    expect(config).toEqual({
      host: '127.0.0.1',
      port: 3001,
    });
  });

  it('uses BACKEND_HOST and BACKEND_PORT when provided', () => {
    const config = getServerConfig({
      BACKEND_HOST: '0.0.0.0',
      BACKEND_PORT: '4001',
    });

    expect(config).toEqual({
      host: '0.0.0.0',
      port: 4001,
    });
  });

  it('falls back to default port when BACKEND_PORT is invalid', () => {
    const config = getServerConfig({
      BACKEND_PORT: 'not-a-number',
    });

    expect(config).toEqual({
      host: '127.0.0.1',
      port: 3001,
    });
  });
});
