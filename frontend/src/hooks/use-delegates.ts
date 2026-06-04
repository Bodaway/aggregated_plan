import { useQuery } from 'urql';

const DELEGATES_QUERY = `
  query Delegates {
    delegates
  }
`;

/** Auto-learned list of names previously used in the delegated-to field. */
export function useDelegates() {
  const [result] = useQuery<{ delegates: string[] }>({
    query: DELEGATES_QUERY,
    requestPolicy: 'cache-and-network',
  });
  return { delegates: result.data?.delegates ?? [] };
}
