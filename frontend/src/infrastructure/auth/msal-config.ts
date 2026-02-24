import type { Configuration, PopupRequest } from '@azure/msal-browser';

export const createMsalConfig = (): Configuration => ({
  auth: {
    clientId: import.meta.env.VITE_AZURE_AD_CLIENT_ID?.toString() ?? '',
    authority: `https://login.microsoftonline.com/${import.meta.env.VITE_AZURE_AD_TENANT_ID?.toString() ?? 'common'}`,
    redirectUri: import.meta.env.VITE_AZURE_AD_REDIRECT_URI?.toString() ?? 'http://localhost:3000',
  },
  cache: {
    cacheLocation: 'sessionStorage',
    storeAuthStateInCookie: false,
  },
});

export const createLoginRequest = (): PopupRequest => ({
  scopes: [
    import.meta.env.VITE_AZURE_AD_API_SCOPE?.toString() ?? 'User.Read',
  ],
});

export const isMsalEnabled = (): boolean =>
  Boolean(
    import.meta.env.VITE_AZURE_AD_CLIENT_ID &&
    import.meta.env.VITE_AZURE_AD_TENANT_ID,
  );
