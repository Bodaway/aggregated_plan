import { useMsal } from '@azure/msal-react';
import { useCallback } from 'react';
import { createLoginRequest } from './msal-config';

type UseAccessTokenResult = {
  readonly getToken: () => Promise<string>;
};

export const useAccessToken = (): UseAccessTokenResult => {
  const { instance, accounts } = useMsal();

  const getToken = useCallback(async (): Promise<string> => {
    const account = accounts[0];
    if (!account) {
      throw new Error('No authenticated account found');
    }

    const request = {
      ...createLoginRequest(),
      account,
    };

    const response = await instance.acquireTokenSilent(request);
    return response.accessToken;
  }, [instance, accounts]);

  return { getToken };
};
