import React from 'react';
import { PublicClientApplication } from '@azure/msal-browser';
import { MsalProvider } from '@azure/msal-react';
import { createMsalConfig, isMsalEnabled } from './msal-config';

type AuthProviderProps = {
  readonly children: React.ReactNode;
};

// PublicClientApplication from @azure/msal-browser requires `new` —
// third-party SDK exception to the "no classes" rule.
const msalInstance = isMsalEnabled()
  ? new PublicClientApplication(createMsalConfig())
  : null;

export const AuthProvider = ({ children }: AuthProviderProps): React.JSX.Element => {
  if (!msalInstance) {
    return <>{children}</>;
  }
  return <MsalProvider instance={msalInstance}>{children}</MsalProvider>;
};

export const getMsalInstance = (): PublicClientApplication | null => msalInstance;
