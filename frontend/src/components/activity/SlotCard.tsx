interface SlotCardProps {
  readonly id: string;
  readonly taskTitle: string | null;
  readonly startTime: string;
  readonly endTime: string | null;
  readonly halfDay: string;
  readonly durationMinutes: number | null;
  readonly onDelete: (id: string) => void;
  readonly onEdit?: (id: string) => void;
}

/** Formats a time string (ISO or HH:mm) into a display-friendly HH:mm format. */
function formatTime(timeStr: string): string {
  try {
    const d = new Date(timeStr);
    if (!isNaN(d.getTime())) {
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
    }
  } catch {
    // fall through
  }
  return timeStr;
}

/** Formats duration in minutes to a human-readable string. */
function formatDuration(minutes: number): string {
  const hours = Math.floor(minutes / 60);
  const mins = minutes % 60;
  if (hours === 0) return `${mins}min`;
  if (mins === 0) return `${hours}h`;
  return `${hours}h ${mins}min`;
}

function getHalfDayBadge(halfDay: string): { label: string; className: string } {
  if (halfDay === 'AM') {
    return { label: 'AM', className: 'bg-amber-100 text-amber-700' };
  }
  return { label: 'PM', className: 'bg-indigo-100 text-indigo-700' };
}

export function SlotCard({
  id,
  taskTitle,
  startTime,
  endTime,
  halfDay,
  durationMinutes,
  onDelete,
  onEdit,
}: SlotCardProps) {
  const badge = getHalfDayBadge(halfDay);

  return (
    <div className="flex items-center gap-3 p-3 bg-white rounded-lg border border-gray-200 hover:shadow-sm transition-shadow">
      {/* Time range */}
      <div className="flex-shrink-0 text-center w-20">
        <p className="text-sm font-medium text-gray-800">{formatTime(startTime)}</p>
        {endTime && (
          <>
            <div className="w-px h-2 bg-gray-300 mx-auto" />
            <p className="text-sm font-medium text-gray-800">{formatTime(endTime)}</p>
          </>
        )}
      </div>

      {/* Divider */}
      <div className="w-px h-10 bg-gray-200 flex-shrink-0" />

      {/* Task info */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span
            className={`inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium ${badge.className}`}
          >
            {badge.label}
          </span>
          <p className="text-sm font-medium text-gray-800 truncate">
            {taskTitle ?? 'Untitled activity'}
          </p>
        </div>
        {durationMinutes !== null && durationMinutes > 0 && (
          <p className="text-xs text-gray-500 mt-0.5">
            Duration: {formatDuration(durationMinutes)}
          </p>
        )}
      </div>

      {/* Action buttons */}
      <div className="flex items-center gap-1 flex-shrink-0">
        <button
          type="button"
          onClick={() => onEdit?.(id)}
          className="p-1.5 text-gray-400 hover:text-blue-500 hover:bg-blue-50 rounded-md transition-colors"
          aria-label="Edit activity slot"
          title="Edit this activity slot"
        >
          <svg
            className="w-4 h-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10"
            />
          </svg>
        </button>
        <button
          type="button"
          onClick={() => onDelete(id)}
          className="p-1.5 text-gray-400 hover:text-red-500 hover:bg-red-50 rounded-md transition-colors"
          aria-label="Delete activity slot"
          title="Delete this activity slot"
        >
          <svg
            className="w-4 h-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"
            />
          </svg>
        </button>
      </div>
    </div>
  );
}
