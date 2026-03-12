import { AlertBadge } from './AlertBadge';

interface Alert {
  readonly id: string;
  readonly alertType: string;
  readonly severity: string;
  readonly message: string;
  readonly resolved: boolean;
  readonly createdAt: string;
}

interface AlertPanelProps {
  readonly alerts: readonly Alert[];
}

const SEVERITY_ORDER: Record<string, number> = {
  CRITICAL: 0,
  WARNING: 1,
  INFORMATION: 2,
};

function getSeverityOrder(severity: string): number {
  return SEVERITY_ORDER[severity] ?? 3;
}

function countBySeverity(
  alerts: readonly Alert[]
): readonly { severity: string; count: number }[] {
  const counts = new Map<string, number>();
  alerts.forEach(alert => {
    if (!alert.resolved) {
      counts.set(alert.severity, (counts.get(alert.severity) ?? 0) + 1);
    }
  });

  return [...counts.entries()]
    .sort((a, b) => getSeverityOrder(a[0]) - getSeverityOrder(b[0]))
    .map(([severity, count]) => ({ severity, count }));
}

const SEVERITY_BADGE_STYLES: Record<string, string> = {
  CRITICAL: 'bg-red-100 text-red-700',
  WARNING: 'bg-yellow-100 text-yellow-700',
  INFORMATION: 'bg-blue-100 text-blue-700',
};

export function AlertPanel({ alerts }: AlertPanelProps) {
  const unresolvedAlerts = alerts.filter(a => !a.resolved);
  const severityCounts = countBySeverity(alerts);

  if (alerts.length === 0) {
    return (
      <div className="text-gray-500 text-sm py-4 text-center">
        No alerts
      </div>
    );
  }

  const sortedAlerts = [...unresolvedAlerts].sort(
    (a, b) => getSeverityOrder(a.severity) - getSeverityOrder(b.severity)
  );

  return (
    <div>
      {/* Severity summary badges */}
      {severityCounts.length > 0 && (
        <div className="flex items-center gap-2 mb-3">
          {severityCounts.map(({ severity, count }) => (
            <span
              key={severity}
              className={`inline-flex items-center gap-1 px-2 py-1 rounded-full text-xs font-medium ${
                SEVERITY_BADGE_STYLES[severity] ?? 'bg-gray-100 text-gray-700'
              }`}
            >
              {severity.toLowerCase()}: {count}
            </span>
          ))}
        </div>
      )}

      {/* Alert list */}
      <div className="space-y-2">
        {sortedAlerts.map(alert => (
          <AlertBadge
            key={alert.id}
            id={alert.id}
            alertType={alert.alertType}
            severity={alert.severity}
            message={alert.message}
            resolved={alert.resolved}
            createdAt={alert.createdAt}
          />
        ))}
      </div>

      {unresolvedAlerts.length === 0 && alerts.length > 0 && (
        <p className="text-gray-500 text-sm py-2 text-center">
          All alerts resolved
        </p>
      )}
    </div>
  );
}
