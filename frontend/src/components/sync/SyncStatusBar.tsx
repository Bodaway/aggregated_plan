interface SyncStatusItem {
  readonly source: string;
  readonly status: string;
  readonly lastSyncAt: string | null;
  readonly errorMessage: string | null;
}

interface SyncStatusBarProps {
  readonly statuses: readonly SyncStatusItem[];
}

function getStatusDotColor(status: string): string {
  switch (status) {
    case 'SUCCESS':
    case 'SYNCED':
      return '#10B981'; // green
    case 'SYNCING':
    case 'IN_PROGRESS':
      return '#F59E0B'; // yellow
    case 'ERROR':
    case 'FAILED':
      return '#EF4444'; // red
    case 'IDLE':
    default:
      return '#9CA3AF'; // gray
  }
}

function getStatusLabel(status: string): string {
  switch (status) {
    case 'SUCCESS':
    case 'SYNCED':
      return 'Synced';
    case 'SYNCING':
    case 'IN_PROGRESS':
      return 'Syncing...';
    case 'ERROR':
    case 'FAILED':
      return 'Error';
    case 'IDLE':
    default:
      return 'Idle';
  }
}

function formatLastSync(lastSyncAt: string | null): string {
  if (!lastSyncAt) return 'Never';
  try {
    const date = new Date(lastSyncAt);
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  } catch {
    return lastSyncAt;
  }
}

export function SyncStatusBar({ statuses }: SyncStatusBarProps) {
  if (statuses.length === 0) {
    return null;
  }

  return (
    <div className="flex items-center gap-4 px-4 py-2 bg-white border border-gray-200 rounded-lg">
      <span className="text-xs font-medium text-gray-500 uppercase tracking-wider">Sync</span>
      <div className="flex items-center gap-4">
        {statuses.map(item => {
          const dotColor = getStatusDotColor(item.status);
          const label = getStatusLabel(item.status);
          const lastSync = formatLastSync(item.lastSyncAt);

          return (
            <div
              key={item.source}
              className="flex items-center gap-1.5"
              title={
                item.errorMessage
                  ? `${item.source}: ${item.errorMessage}`
                  : `${item.source}: Last sync ${lastSync}`
              }
            >
              <span
                className="inline-block w-2 h-2 rounded-full flex-shrink-0"
                style={{ backgroundColor: dotColor }}
              />
              <span className="text-xs text-gray-600">{item.source}</span>
              <span className="text-xs text-gray-400">({label})</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
