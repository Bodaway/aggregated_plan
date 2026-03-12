import { SEVERITY_COLORS } from '@/lib/constants';

interface AlertBadgeProps {
  readonly id: string;
  readonly alertType: string;
  readonly severity: string;
  readonly message: string;
  readonly resolved: boolean;
  readonly createdAt: string;
}

function getSeverityStyle(severity: string): { bg: string; text: string; border: string } {
  switch (severity) {
    case 'CRITICAL':
      return { bg: 'bg-red-50', text: 'text-red-800', border: 'border-red-200' };
    case 'WARNING':
      return { bg: 'bg-yellow-50', text: 'text-yellow-800', border: 'border-yellow-200' };
    case 'INFORMATION':
      return { bg: 'bg-blue-50', text: 'text-blue-800', border: 'border-blue-200' };
    default:
      return { bg: 'bg-gray-50', text: 'text-gray-800', border: 'border-gray-200' };
  }
}

function getSeverityDotColor(severity: string): string {
  return (SEVERITY_COLORS as Record<string, string>)[severity] ?? '#6B7280';
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

export function AlertBadge({
  alertType,
  severity,
  message,
  resolved,
}: AlertBadgeProps) {
  const style = getSeverityStyle(severity);
  const dotColor = getSeverityDotColor(severity);
  const iconPath = getSeverityIcon(severity);

  return (
    <div
      className={`flex items-start gap-2 p-3 rounded-lg border ${style.bg} ${style.border} ${
        resolved ? 'opacity-50' : ''
      }`}
    >
      <svg
        className="w-4 h-4 flex-shrink-0 mt-0.5"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={1.5}
        style={{ color: dotColor }}
      >
        <path strokeLinecap="round" strokeLinejoin="round" d={iconPath} />
      </svg>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className={`text-xs font-semibold uppercase ${style.text}`}>{alertType}</span>
          {resolved && (
            <span className="text-xs text-gray-500 italic">(resolved)</span>
          )}
        </div>
        <p className={`text-sm mt-0.5 ${style.text}`}>{message}</p>
      </div>
    </div>
  );
}
