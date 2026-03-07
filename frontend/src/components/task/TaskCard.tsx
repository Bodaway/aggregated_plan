import { SOURCE_COLORS, QUADRANT_LABELS } from '@/lib/constants';

interface TaskTag {
  readonly id: string;
  readonly name: string;
  readonly color?: string | null;
}

export interface TaskCardProps {
  readonly id: string;
  readonly title: string;
  readonly source: string;
  readonly status: string;
  readonly urgency: number;
  readonly impact: number;
  readonly quadrant: string;
  readonly deadline?: string | null;
  readonly assignee?: string | null;
  readonly projectName?: string | null;
  readonly tags?: readonly TaskTag[];
}

const STATUS_STYLES: Record<string, string> = {
  TODO: 'bg-gray-100 text-gray-700',
  IN_PROGRESS: 'bg-blue-100 text-blue-700',
  DONE: 'bg-green-100 text-green-700',
  BLOCKED: 'bg-red-100 text-red-700',
  CANCELLED: 'bg-gray-200 text-gray-500',
};

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

export function TaskCard({
  title,
  source,
  status,
  quadrant,
  deadline,
  assignee,
  projectName,
  tags,
}: TaskCardProps) {
  const sourceColor = getSourceColor(source);
  const statusStyle = STATUS_STYLES[status] ?? 'bg-gray-100 text-gray-700';
  const quadrantStyle = QUADRANT_STYLES[quadrant] ?? 'bg-gray-100 text-gray-600';
  const quadrantLabel = getQuadrantLabel(quadrant);

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4 hover:shadow-sm transition-shadow">
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-2">
            <span
              className="inline-block w-2 h-2 rounded-full flex-shrink-0"
              style={{ backgroundColor: sourceColor }}
              title={source}
            />
            <h3 className="text-sm font-medium text-gray-900 truncate">{title}</h3>
          </div>

          <div className="flex flex-wrap items-center gap-1.5 mb-2">
            <span className={`inline-flex px-2 py-0.5 rounded text-xs font-medium ${statusStyle}`}>
              {status.replace('_', ' ')}
            </span>
            <span
              className={`inline-flex px-2 py-0.5 rounded text-xs font-medium ${quadrantStyle}`}
            >
              {quadrantLabel}
            </span>
          </div>

          <div className="flex flex-wrap items-center gap-3 text-xs text-gray-500">
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
