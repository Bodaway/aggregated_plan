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
      return 'var(--cn-green)'; // green
    case 'SYNCING':
    case 'IN_PROGRESS':
      return 'var(--cn-yellow)'; // yellow
    case 'ERROR':
    case 'FAILED':
      return 'var(--cn-red)'; // red
    case 'NOT_CONFIGURED':
      return 'var(--app-ink-low)'; // grey: nothing is wrong, nothing is configured either
    case 'IDLE':
    default:
      return 'var(--app-ink-low)'; // gray
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
    case 'NOT_CONFIGURED':
      return 'Non configuré';
    case 'IDLE':
    default:
      return 'Idle';
  }
}

/** Gryzzly's credential is a browser cookie with a fixed 7-day life, so
 *  re-logging-in is a weekly chore rather than an incident. That is why this one
 *  source gets a direct link and the others do not. */
function needsGryzzlyReconnect(source: string, status: string): boolean {
  return source.toUpperCase() === 'GRYZZLY' && (status === 'ERROR' || status === 'NOT_CONFIGURED');
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

  const needingAttention = statuses.filter(
    item => item.errorMessage || needsGryzzlyReconnect(item.source, item.status),
  );

  return (
    <div className="flex flex-col gap-2 px-4 py-2 bg-white border border-gray-200 rounded-lg">
      <div className="flex items-center gap-4">
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

      {/* Reasons, inline. A tooltip hides the one piece of text that tells you
          what to do — most often "the session expired on <date>, log in again". */}
      {needingAttention.length > 0 && (
        <div className="flex flex-col gap-1 pt-1 border-t border-gray-100">
          {needingAttention.map(item => (
            <div key={`${item.source}-reason`} className="flex items-start gap-1.5 text-xs">
              <span className="font-medium text-gray-500 flex-shrink-0">{item.source}</span>
              {item.errorMessage && <span className="text-gray-600">{item.errorMessage}</span>}
              {needsGryzzlyReconnect(item.source, item.status) && (
                <a
                  href="https://app.gryzzly.io"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-blue-600 hover:underline flex-shrink-0"
                >
                  Reconnecter
                </a>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
