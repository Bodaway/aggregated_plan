import { useCallback } from 'react';
import { useMutation, useQuery } from 'urql';

const SESSION_QUERY = `query Session { session { authenticated account } }`;
const SIGN_OUT_MUTATION = `mutation SignOut { signOut }`;

export interface SessionData { authenticated: boolean; account: string | null; }

export function useSession() {
  const [result, reexecute] = useQuery<{ session: SessionData }>({ query: SESSION_QUERY });
  const [, executeSignOut] = useMutation<{ signOut: boolean }>(SIGN_OUT_MUTATION);
  const signOut = useCallback(async () => {
    await executeSignOut({});
    reexecute({ requestPolicy: 'network-only' });
  }, [executeSignOut, reexecute]);
  const refresh = useCallback(
    () => reexecute({ requestPolicy: 'network-only' }),
    [reexecute]
  );
  return {
    session: result.data?.session ?? { authenticated: false, account: null },
    fetching: result.fetching,
    error: result.error ?? null,
    refresh,
    signOut,
  };
}
