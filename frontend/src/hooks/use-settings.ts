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
  readonly outlookConnection: { readonly connected: boolean; readonly account: string | null };
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
    outlookConnection { connected account }
  }
`;

const UPDATE_CONFIGURATION_MUTATION = `
  mutation UpdateConfiguration($key: String!, $value: String!) {
    updateConfiguration(key: $key, value: $value)
  }
`;

const FORCE_SYNC_MUTATION = `
  mutation ForceSync($source: SourceGql) {
    forceSync(source: $source) {
      source
      status
      lastSyncAt
      errorMessage
    }
  }
`;

const DISCONNECT_OUTLOOK_MUTATION = `
  mutation DisconnectOutlook { disconnectOutlook }
`;

interface ForceSyncResult {
  readonly forceSync: readonly SyncStatusData[];
}

export function useSettings() {
  const [result, reexecute] = useQuery<ConfigurationData>({
    query: CONFIGURATION_QUERY,
  });

  const [, executeUpdateConfig] = useMutation<{ updateConfiguration: boolean }>(
    UPDATE_CONFIGURATION_MUTATION
  );

  const [syncResult, executeForceSync] = useMutation<ForceSyncResult>(FORCE_SYNC_MUTATION);

  const [, executeDisconnectOutlook] = useMutation<{ disconnectOutlook: boolean }>(
    DISCONNECT_OUTLOOK_MUTATION
  );

  const configuration = useMemo(
    () => result.data?.configuration ?? {},
    [result.data?.configuration]
  );

  const syncStatuses = useMemo(
    () => result.data?.syncStatuses ?? [],
    [result.data?.syncStatuses]
  );

  const outlookConnection = useMemo(
    () => result.data?.outlookConnection ?? { connected: false, account: null },
    [result.data?.outlookConnection]
  );

  const disconnectOutlook = useCallback(async () => {
    const res = await executeDisconnectOutlook({});
    if (!res.error) reexecute({ requestPolicy: 'network-only' });
    return res;
  }, [executeDisconnectOutlook, reexecute]);

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
    outlookConnection,
    loading: result.fetching,
    error: result.error ?? null,
    syncing: syncResult.fetching,
    updateConfig,
    forceSync,
    disconnectOutlook,
    refetch: () => reexecute({ requestPolicy: 'network-only' }),
  };
}
