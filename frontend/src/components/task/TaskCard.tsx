import { SOURCE_COLORS, QUADRANT_LABELS } from '@/lib/constants';
import { useSearch } from '@/lib/search/SearchProvider';
import { StatusMenu } from './StatusMenu';

interface TaskTag {
  readonly id: string;
  readonly name: string;
  readonly color?: string | null;
}

export interface TaskCardProps {
  readonly id: string;
  readonly title: string;
  readonly source: string;
  readonly sourceId?: string | null;
  readonly status: string;
  readonly jiraStatus?: string | null;
  readonly urgency: number;
  readonly impact: number;
  readonly quadrant: string;
  readonly deadline?: string | null;
  readonly assignee?: string | null;
  readonly projectName?: string | null;
  readonly tags?: readonly TaskTag[];
  readonly effectiveRemainingHours?: number | null;
  readonly effectiveEstimatedHours?: number | null;
  readonly jiraTimeSpentSeconds?: number | null;
  readonly isRecurring?: boolean;
  readonly compact?: boolean;
  readonly onClick?: () => void;
}


const QUADRANT_STYLES: Record<string, string> = {
  UrgentImportant: 'bg-red-100 text-red-800',
  Important: 'bg-yellow-100 text-yellow-800',
  Urgent: 'bg-orange-100 text-orange-800',
  Neither: 'bg-gray-100 text-gray-600',
};

function getSourceColor(source: string): string {
  return (SOURCE_COLORS as Record<string, string>)[source] ?? SOURCE_COLORS.PERSONAL;
}

function getQuadrantLabel(quadrant: string): string {
  return (QUADRANT_LABELS as Record<string, string>)[quadrant] ?? quadrant;
}

function formatHours(hours: number | null | undefined): string {
  if (hours === null || hours === undefined) return '-';
  if (hours < 1) return `${Math.round(hours * 60)}m`;
  return `${hours.toFixed(1)}h`;
}

function TimeTrackingRow({
  remaining,
  logged,
  estimate,
}: {
  readonly remaining: number | null | undefined;
  readonly logged: number | null | undefined;
  readonly estimate: number | null | undefined;
}) {
  if (remaining == null && logged == null && estimate == null) return null;

  const loggedHours = logged !== null && logged !== undefined ? logged / 3600 : null;
  const progressPct = estimate && loggedHours !== null ? Math.min((loggedHours / estimate) * 100, 100) : null;

  return (
    <div className="flex items-center gap-2 text-xs text-gray-500 mt-1">
      <svg className="w-3.5 h-3.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      <span title="Remaining">{formatHours(remaining)}</span>
      <span className="text-gray-300">/</span>
      <span title="Logged">{formatHours(loggedHours)}</span>
      <span className="text-gray-300">/</span>
      <span title="Estimate">{formatHours(estimate)}</span>
      {progressPct !== null && (
        <div className="flex-1 h-1.5 bg-gray-200 rounded-full overflow-hidden max-w-16">
          <div
            className={`h-full rounded-full ${progressPct >= 90 ? 'bg-red-400' : progressPct >= 70 ? 'bg-yellow-400' : 'bg-blue-400'}`}
            style={{ width: `${progressPct}%` }}
          />
        </div>
      )}
    </div>
  );
}

function urgencyBorderClass(urgency: number): string {
  if (urgency >= 4) return 'border-l-red-600';   // Critical
  if (urgency === 3) return 'border-l-orange-600'; // High
  if (urgency === 2) return 'border-l-yellow-600'; // Medium
  return 'border-l-gray-400';                      // Low (1) and fallback
}

export function TaskCard({
  id,
  title,
  source,
  sourceId,
  status,
  jiraStatus,
  urgency,
  quadrant,
  deadline,
  assignee,
  projectName,
  tags,
  effectiveRemainingHours,
  effectiveEstimatedHours,
  jiraTimeSpentSeconds,
  isRecurring = false,
  compact = false,
  onClick,
}: TaskCardProps) {
  const { highlightActive, matchedIds } = useSearch();
  const isMatch = matchedIds.has(id);
  const highlightClasses = !highlightActive
    ? ''
    : isMatch
      ? 'ring-2 ring-blue-500 ring-offset-2'
      : 'opacity-40 grayscale-[30%]';
  const sourceColor = getSourceColor(source);

  if (compact) {
    return (
      <div
        data-testid="task-card-root"
        className={`bg-white rounded-md border border-gray-200 border-l-4 ${urgencyBorderClass(urgency)} p-2.5 hover:shadow-sm transition-shadow ${onClick ? 'cursor-pointer' : ''} ${highlightClasses}`}
        onClick={onClick}
      >
        {/* Top row: source ID + remaining hours */}
        <div className="flex items-center justify-between gap-1 mb-1">
          <div className="flex items-center gap-1.5">
            <span
              className="inline-block w-2 h-2 rounded-full flex-shrink-0"
              style={{ backgroundColor: sourceColor }}
            />
            {sourceId && (
              <span className="text-xs font-mono font-medium text-blue-600">{sourceId}</span>
            )}
          </div>
          {effectiveRemainingHours !== null && effectiveRemainingHours !== undefined && (
            <span className="text-xs text-gray-500">{formatHours(effectiveRemainingHours)}</span>
          )}
        </div>
        {/* Title */}
        <h4 className="text-sm font-medium text-gray-900 mb-1 leading-tight truncate">{title}</h4>
        {/* Bottom row: status menu + assignee + recurring icon */}
        <div className="flex flex-wrap items-center gap-1.5">
          <StatusMenu taskId={id} status={status} />
          {assignee && (
            <span className="text-xs text-gray-400 truncate">{assignee}</span>
          )}
          {isRecurring && (
            <span title="Tâche récurrente" className="inline-flex items-center">
              <svg className="w-3 h-3 text-violet-600" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-label="Tâche récurrente" role="img">
                <path d="M17 2l4 4-4 4" /><path d="M3 11V9a4 4 0 014-4h14" />
                <path d="M7 22l-4-4 4-4" /><path d="M21 13v2a4 4 0 01-4 4H3" />
              </svg>
            </span>
          )}
        </div>
      </div>
    );
  }

  // Full card
  const quadrantStyle = QUADRANT_STYLES[quadrant] ?? 'bg-gray-100 text-gray-600';
  const quadrantLabel = getQuadrantLabel(quadrant);

  return (
    <div
      data-testid="task-card-root"
      className={`bg-white rounded-lg border border-gray-200 border-l-4 ${urgencyBorderClass(urgency)} p-4 hover:shadow-sm transition-shadow ${onClick ? 'cursor-pointer' : ''} ${highlightClasses}`}
      onClick={onClick}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-2">
            <span
              className="inline-block w-2 h-2 rounded-full flex-shrink-0"
              style={{ backgroundColor: sourceColor }}
              title={source}
            />
            {sourceId && (
              <span className="text-xs font-mono font-medium text-blue-600 flex-shrink-0">
                {sourceId}
              </span>
            )}
            <h3 className="text-sm font-medium text-gray-900 truncate">{title}</h3>
          </div>

          <div className="flex flex-wrap items-center gap-1.5 mb-2">
            <StatusMenu taskId={id} status={status} />
            {jiraStatus && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-medium bg-blue-50 text-blue-700 border border-blue-200">
                <svg className="w-3 h-3 flex-shrink-0" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M11.53 2c0 2.4 1.97 4.35 4.35 4.35h1.78v1.7c0 2.4 1.94 4.34 4.34 4.35V2.84a.84.84 0 00-.84-.84H11.53zM6.77 6.8a4.36 4.36 0 004.34 4.34h1.78v1.72a4.36 4.36 0 004.34 4.34V7.63a.84.84 0 00-.83-.83H6.77zM2 11.6c0 2.4 1.95 4.34 4.35 4.34h1.78v1.72c0 2.4 1.94 4.34 4.35 4.34v-9.57a.84.84 0 00-.84-.83H2z" />
                </svg>
                {jiraStatus}
              </span>
            )}
            <span className={`inline-flex px-2 py-0.5 rounded text-xs font-medium ${quadrantStyle}`}>
              {quadrantLabel}
            </span>
          </div>

          {/* Time tracking row */}
          <TimeTrackingRow
            remaining={effectiveRemainingHours}
            logged={jiraTimeSpentSeconds}
            estimate={effectiveEstimatedHours}
          />

          <div className="flex flex-wrap items-center gap-3 text-xs text-gray-500 mt-1">
            {isRecurring && (
              <span title="Tâche récurrente" className="inline-flex items-center gap-0.5">
                <svg className="w-3 h-3 text-violet-600 inline-block" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-label="Tâche récurrente" role="img">
                  <polyline points="17 1 21 5 17 9" />
                  <path d="M3 11V9a4 4 0 0 1 4-4h14" />
                  <polyline points="7 23 3 19 7 15" />
                  <path d="M21 13v2a4 4 0 0 1-4 4H3" />
                </svg>
              </span>
            )}
            {deadline && (
              <span className="flex items-center gap-1">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5" />
                </svg>
                {deadline}
              </span>
            )}
            {assignee && (
              <span className="flex items-center gap-1">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z" />
                </svg>
                {assignee}
              </span>
            )}
            {projectName && (
              <span className="flex items-center gap-1">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z" />
                </svg>
                {projectName}
              </span>
            )}
          </div>

          {tags && tags.length > 0 && (
            <div className="flex flex-wrap gap-1 mt-2">
              {tags.map(tag => (
                <span
                  key={tag.id}
                  className="inline-flex px-1.5 py-0.5 rounded text-xs font-medium"
                  style={{
                    backgroundColor: tag.color ? `${tag.color}20` : '#E5E7EB',
                    color: tag.color ?? '#4B5563',
                  }}
                >
                  {tag.name}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
