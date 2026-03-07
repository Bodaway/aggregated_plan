import { useCallback, useMemo } from 'react';
import { useQuery, useMutation } from 'urql';

export interface SyncStatusData {
  readonly source: string;
  readonly status: string;
  readonly lastSyncAt: string | null;
  readonly errorMessage: string | null;
}

export interface ConfigurationData {
  readonly configuration: Record<string, string>;
  readonly syncStatuses: readonly SyncStatusData[];
}

const CONFIGURATION_QUERY = `
  query Configuration {
    configuration
    syncStatuses {
      source
      status
      lastSyncAt
      errorMessage
    }
  }
`;

const UPDATE_CONFIGURATION_MUTATION = `
  mutation UpdateConfiguration($key: String!, $value: String!) {
    updateConfiguration(key: $key, value: $value)
  }
`;

const FORCE_SYNC_MUTATION = `
  mutation ForceSync($source: String) {
    forceSync(source: $source) {
      source
      status
      lastSyncAt
      errorMessage
    }
  }
`;

interface ForceSyncResult {
  readonly forceSync: SyncStatusData;
}

export function useSettings() {
  const [result, reexecute] = useQuery<ConfigurationData>({
    query: CONFIGURATION_QUERY,
  });

  const [, executeUpdateConfig] = useMutation<{ updateConfiguration: boolean }>(
    UPDATE_CONFIGURATION_MUTATION
  );

  const [syncResult, executeForceSync] = useMutation<ForceSyncResult>(FORCE_SYNC_MUTATION);

  const configuration = useMemo(
    () => result.data?.configuration ?? {},
    [result.data?.configuration]
  );

  const syncStatuses = useMemo(
    () => result.data?.syncStatuses ?? [],
    [result.data?.syncStatuses]
  );

  const updateConfig = useCallback(
    async (key: string, value: string) => {
      const res = await executeUpdateConfig({ key, value });
      if (!res.error) {
        reexecute({ requestPolicy: 'network-only' });
      }
      return res;
    },
    [executeUpdateConfig, reexecute]
  );

  const forceSync = useCallback(
    async (source?: string) => {
      const res = await executeForceSync({ source: source ?? null });
      if (!res.error) {
        reexecute({ requestPolicy: 'network-only' });
      }
      return res;
    },
    [executeForceSync, reexecute]
  );

  return {
    configuration,
    syncStatuses,
    loading: result.fetching,
    error: result.error ?? null,
    syncing: syncResult.fetching,
    updateConfig,
    forceSync,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
