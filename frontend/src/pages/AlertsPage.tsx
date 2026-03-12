import { useState, useCallback } from 'react';
import { useAlerts } from '@/hooks/use-alerts';
import type { AlertFilter, AlertNode } from '@/hooks/use-alerts';
import { SEVERITY_COLORS } from '@/lib/constants';

/** Tab definitions for filter navigation. */
const FILTER_TABS: readonly { readonly key: AlertFilter; readonly label: string }[] = [
  { key: 'all', label: 'All' },
  { key: 'unresolved', label: 'Unresolved' },
  { key: 'resolved', label: 'Resolved' },
];

/** Severity display configuration. */
const SEVERITY_CONFIG: Record<string, { bg: string; text: string; border: string; badge: string }> =
  {
    CRITICAL: {
      bg: 'bg-red-50',
      text: 'text-red-800',
      border: 'border-red-200',
      badge: 'bg-red-100 text-red-700',
    },
    WARNING: {
      bg: 'bg-yellow-50',
      text: 'text-yellow-800',
      border: 'border-yellow-200',
      badge: 'bg-yellow-100 text-yellow-700',
    },
    INFORMATION: {
      bg: 'bg-blue-50',
      text: 'text-blue-800',
      border: 'border-blue-200',
      badge: 'bg-blue-100 text-blue-700',
    },
  };

const DEFAULT_SEVERITY_CONFIG = {
  bg: 'bg-gray-50',
  text: 'text-gray-800',
  border: 'border-gray-200',
  badge: 'bg-gray-100 text-gray-700',
};

function getSeverityConfig(severity: string) {
  return SEVERITY_CONFIG[severity] ?? DEFAULT_SEVERITY_CONFIG;
}

function getSeverityIcon(severity: string): string {
  switch (severity) {
    case 'CRITICAL':
      return 'M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z';
    case 'WARNING':
      return 'M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z';
    default:
      return 'M11.25 11.25l.041-.02a.75.75 0 011.063.852l-.708 2.836a.75.75 0 001.063.853l.041-.021M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9-3.75h.008v.008H12V8.25z';
  }
}

/** Format a date string for display. */
function formatAlertDate(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    return d.toLocaleDateString([], {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return dateStr;
  }
}

/** Summary stat card. */
function StatCard({
  label,
  count,
  color,
}: {
  readonly label: string;
  readonly count: number;
  readonly color: string;
}) {
  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4 text-center">
      <p className={`text-2xl font-bold ${color}`}>{count}</p>
      <p className="text-xs text-gray-500 mt-1">{label}</p>
    </div>
  );
}

/** Individual alert row. */
function AlertRow({
  alert,
  onResolve,
  resolving,
}: {
  readonly alert: AlertNode;
  readonly onResolve: (id: string) => void;
  readonly resolving: boolean;
}) {
  const config = getSeverityConfig(alert.severity);
  const iconPath = getSeverityIcon(alert.severity);
  const dotColor = (SEVERITY_COLORS as Record<string, string>)[alert.severity] ?? '#6B7280';

  return (
    <div
      className={`flex items-start gap-3 p-4 rounded-lg border ${config.bg} ${config.border} ${
        alert.resolved ? 'opacity-60' : ''
      }`}
    >
      {/* Severity icon */}
      <svg
        className="w-5 h-5 flex-shrink-0 mt-0.5"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={1.5}
        style={{ color: dotColor }}
      >
        <path strokeLinecap="round" strokeLinejoin="round" d={iconPath} />
      </svg>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-1">
          <span className={`text-xs font-semibold uppercase ${config.text}`}>
            {alert.alertType}
          </span>
          <span className={`inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium ${config.badge}`}>
            {alert.severity}
          </span>
          {alert.resolved && (
            <span className="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-green-100 text-green-700">
              Resolved
            </span>
          )}
        </div>
        <p className={`text-sm ${config.text}`}>{alert.message}</p>
        <p className="text-xs text-gray-400 mt-1">{formatAlertDate(alert.createdAt)}</p>
      </div>

      {/* Resolve button */}
      {!alert.resolved && (
        <button
          type="button"
          onClick={() => onResolve(alert.id)}
          disabled={resolving}
          className="flex-shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-green-700 bg-green-100 border border-green-200 rounded-md hover:bg-green-200 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          {resolving ? (
            <div className="w-3 h-3 border-2 border-green-500 border-t-transparent rounded-full animate-spin" />
          ) : (
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12.75l6 6 9-13.5" />
            </svg>
          )}
          Resolve
        </button>
      )}
    </div>
  );
}

export function AlertsPage() {
  const [filter, setFilter] = useState<AlertFilter>('unresolved');
  const [resolvingId, setResolvingId] = useState<string | null>(null);
  const { alerts, totalCount, loading, error, resolveAlert, pageInfo, loadMore } =
    useAlerts(filter);

  const handleResolve = useCallback(
    async (id: string) => {
      setResolvingId(id);
      try {
        await resolveAlert(id);
      } finally {
        setResolvingId(null);
      }
    },
    [resolveAlert]
  );

  // Count by severity from current results
  const criticalCount = alerts.filter(a => a.severity === 'CRITICAL' && !a.resolved).length;
  const warningCount = alerts.filter(a => a.severity === 'WARNING' && !a.resolved).length;
  const infoCount = alerts.filter(a => a.severity === 'INFORMATION' && !a.resolved).length;

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load alerts</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4 max-w-4xl">
      {/* Summary stats */}
      <div className="grid grid-cols-3 gap-3">
        <StatCard label="Critical" count={criticalCount} color="text-red-600" />
        <StatCard label="Warning" count={warningCount} color="text-yellow-600" />
        <StatCard label="Information" count={infoCount} color="text-blue-600" />
      </div>

      {/* Filter tabs */}
      <div className="flex items-center gap-1 bg-gray-100 rounded-lg p-1">
        {FILTER_TABS.map(tab => (
          <button
            key={tab.key}
            type="button"
            onClick={() => setFilter(tab.key)}
            className={`flex-1 px-3 py-1.5 text-sm font-medium rounded-md transition-colors ${
              filter === tab.key
                ? 'bg-white text-gray-800 shadow-sm'
                : 'text-gray-500 hover:text-gray-700'
            }`}
          >
            {tab.label}
            {tab.key === 'all' && totalCount > 0 && (
              <span className="ml-1.5 text-xs text-gray-400">({totalCount})</span>
            )}
          </button>
        ))}
      </div>

      {/* Loading state */}
      {loading && alerts.length === 0 && (
        <div className="flex items-center justify-center h-48">
          <div className="text-center">
            <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
            <p className="text-gray-500 text-sm">Loading alerts...</p>
          </div>
        </div>
      )}

      {/* Empty state */}
      {!loading && alerts.length === 0 && (
        <div className="bg-white rounded-lg border border-gray-200 p-12 text-center">
          <svg
            className="w-12 h-12 text-gray-300 mx-auto mb-3"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M14.857 17.082a23.848 23.848 0 005.454-1.31A8.967 8.967 0 0118 9.75v-.7V9A6 6 0 006 9v.75a8.967 8.967 0 01-2.312 6.022c1.733.64 3.56 1.085 5.455 1.31m5.714 0a24.255 24.255 0 01-5.714 0m5.714 0a3 3 0 11-5.714 0"
            />
          </svg>
          <p className="text-gray-500 text-sm font-medium">
            {filter === 'unresolved' ? 'No unresolved alerts' : filter === 'resolved' ? 'No resolved alerts' : 'No alerts'}
          </p>
          <p className="text-gray-400 text-xs mt-1">
            {filter === 'unresolved'
              ? 'All alerts have been resolved. Great job!'
              : 'Alerts will appear here when issues are detected.'}
          </p>
        </div>
      )}

      {/* Alert list */}
      {alerts.length > 0 && (
        <div className="space-y-2">
          {alerts.map(alert => (
            <AlertRow
              key={alert.id}
              alert={alert}
              onResolve={handleResolve}
              resolving={resolvingId === alert.id}
            />
          ))}
        </div>
      )}

      {/* Load more */}
      {pageInfo.hasNextPage && (
        <div className="text-center pt-2">
          <button
            type="button"
            onClick={loadMore}
            disabled={loading}
            className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-gray-600 bg-white border border-gray-300 rounded-md hover:bg-gray-50 disabled:opacity-50 transition-colors"
          >
            {loading ? (
              <div className="w-3.5 h-3.5 border-2 border-gray-400 border-t-transparent rounded-full animate-spin" />
            ) : null}
            Load More
          </button>
        </div>
      )}
    </div>
  );
}
