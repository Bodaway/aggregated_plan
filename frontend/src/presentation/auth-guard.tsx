import React from 'react';
import {
  useIsAuthenticated,
  useMsal,
} from '@azure/msal-react';
import { createLoginRequest, isMsalEnabled } from '../infrastructure/auth/msal-config';

type AuthGuardProps = {
  readonly children: React.ReactNode;
};

export const AuthGuard = ({ children }: AuthGuardProps): React.JSX.Element => {
  if (!isMsalEnabled()) {
    return <>{children}</>;
  }
  return <AuthGuardInner>{children}</AuthGuardInner>;
};

const AuthGuardInner = ({ children }: AuthGuardProps): React.JSX.Element => {
  const isAuthenticated = useIsAuthenticated();
  const { instance } = useMsal();

  const handleLogin = (): void => {
    void instance.loginPopup(createLoginRequest());
  };

  if (!isAuthenticated) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh', flexDirection: 'column', gap: '16px' }}>
        <h2>Aggregated Plan</h2>
        <p>Please sign in to continue</p>
        <button onClick={handleLogin} style={{ padding: '12px 24px', fontSize: '16px', cursor: 'pointer' }}>
          Sign in with Microsoft
        </button>
      </div>
    );
  }

  return <>{children}</>;
};
