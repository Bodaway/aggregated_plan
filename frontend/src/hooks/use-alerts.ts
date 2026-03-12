import { useCallback, useState } from 'react';
import { useQuery, useMutation } from 'urql';

export interface AlertNode {
  readonly id: string;
  readonly alertType: string;
  readonly severity: string;
  readonly message: string;
  readonly date: string | null;
  readonly resolved: boolean;
  readonly createdAt: string;
}

interface AlertEdge {
  readonly node: AlertNode;
}

interface PageInfo {
  readonly hasNextPage: boolean;
  readonly endCursor: string | null;
}

interface AlertsConnection {
  readonly edges: readonly AlertEdge[];
  readonly pageInfo: PageInfo;
  readonly totalCount: number;
}

const ALERTS_QUERY = `
  query Alerts($resolved: Boolean, $first: Int, $after: String) {
    alerts(resolved: $resolved, first: $first, after: $after) {
      edges {
        node {
          id
          alertType
          severity
          message
          date
          resolved
          createdAt
        }
      }
      pageInfo { hasNextPage endCursor }
      totalCount
    }
  }
`;

const RESOLVE_ALERT_MUTATION = `
  mutation ResolveAlert($id: ID!) {
    resolveAlert(id: $id) {
      id resolved
    }
  }
`;

export type AlertFilter = 'all' | 'unresolved' | 'resolved';

export function useAlerts(filter: AlertFilter = 'all', pageSize = 20) {
  const [after, setAfter] = useState<string | null>(null);

  const resolvedVar =
    filter === 'unresolved' ? false : filter === 'resolved' ? true : undefined;

  const [result, reexecute] = useQuery<{ alerts: AlertsConnection }>({
    query: ALERTS_QUERY,
    variables: {
      resolved: resolvedVar ?? null,
      first: pageSize,
      after,
    },
  });

  const [, executeResolve] = useMutation<{ resolveAlert: { id: string; resolved: boolean } }>(
    RESOLVE_ALERT_MUTATION
  );

  const resolveAlert = useCallback(
    async (id: string) => {
      const res = await executeResolve({ id });
      if (!res.error) {
        reexecute({ requestPolicy: 'network-only' });
      }
      return res;
    },
    [executeResolve, reexecute]
  );

  const alerts = result.data?.alerts.edges.map(e => e.node) ?? [];
  const pageInfo = result.data?.alerts.pageInfo ?? { hasNextPage: false, endCursor: null };
  const totalCount = result.data?.alerts.totalCount ?? 0;

  const loadMore = useCallback(() => {
    if (pageInfo.hasNextPage && pageInfo.endCursor) {
      setAfter(pageInfo.endCursor);
    }
  }, [pageInfo]);

  const resetPagination = useCallback(() => {
    setAfter(null);
  }, []);

  return {
    alerts,
    totalCount,
    pageInfo,
    loading: result.fetching,
    error: result.error ?? null,
    resolveAlert,
    loadMore,
    resetPagination,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
