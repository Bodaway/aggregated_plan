import { getAuthConfig, isAuthEnabled } from '../auth-config';

describe('auth-config', () => {
  describe('isAuthEnabled', () => {
    it('returns false when tenant and client are missing', () => {
      expect(isAuthEnabled({})).toBe(false);
    });

    it('returns false when only tenant is set', () => {
      expect(isAuthEnabled({ AZURE_AD_TENANT_ID: 'tenant-123' })).toBe(false);
    });

    it('returns false when only client is set', () => {
      expect(isAuthEnabled({ AZURE_AD_CLIENT_ID: 'client-123' })).toBe(false);
    });

    it('returns true when both tenant and client are set', () => {
      expect(
        isAuthEnabled({
          AZURE_AD_TENANT_ID: 'tenant-123',
          AZURE_AD_CLIENT_ID: 'client-123',
        }),
      ).toBe(true);
    });
  });

  describe('getAuthConfig', () => {
    it('builds config from env vars', () => {
      const config = getAuthConfig({
        AZURE_AD_TENANT_ID: 'my-tenant',
        AZURE_AD_CLIENT_ID: 'my-client',
        AZURE_AD_CLIENT_SECRET: 'my-secret',
        AZURE_AD_SCOPE: 'api://my-client/access_as_user',
      });

      expect(config.tenantId).toBe('my-tenant');
      expect(config.clientId).toBe('my-client');
      expect(config.clientSecret).toBe('my-secret');
      expect(config.scope).toBe('api://my-client/access_as_user');
      expect(config.issuer).toBe('https://login.microsoftonline.com/my-tenant/v2.0');
      expect(config.jwksUri).toBe(
        'https://login.microsoftonline.com/my-tenant/discovery/v2.0/keys',
      );
    });

    it('defaults scope from clientId when not provided', () => {
      const config = getAuthConfig({
        AZURE_AD_TENANT_ID: 'my-tenant',
        AZURE_AD_CLIENT_ID: 'my-client',
      });

      expect(config.scope).toBe('api://my-client/access_as_user');
    });
  });
});
