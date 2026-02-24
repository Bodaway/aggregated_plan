export type AuthConfig = {
  readonly tenantId: string;
  readonly clientId: string;
  readonly clientSecret: string;
  readonly scope: string;
  readonly issuer: string;
  readonly jwksUri: string;
};

export const isAuthEnabled = (env: Record<string, string | undefined>): boolean =>
  Boolean(env['AZURE_AD_TENANT_ID'] && env['AZURE_AD_CLIENT_ID']);

export const getAuthConfig = (env: Record<string, string | undefined>): AuthConfig => {
  const tenantId = env['AZURE_AD_TENANT_ID'] ?? '';
  const clientId = env['AZURE_AD_CLIENT_ID'] ?? '';
  const clientSecret = env['AZURE_AD_CLIENT_SECRET'] ?? '';
  const scope = env['AZURE_AD_SCOPE'] ?? `api://${clientId}/access_as_user`;

  return {
    tenantId,
    clientId,
    clientSecret,
    scope,
    issuer: `https://login.microsoftonline.com/${tenantId}/v2.0`,
    jwksUri: `https://login.microsoftonline.com/${tenantId}/discovery/v2.0/keys`,
  };
};
