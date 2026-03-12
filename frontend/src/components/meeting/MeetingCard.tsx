interface MeetingCardProps {
  readonly title: string;
  readonly startTime: string;
  readonly endTime: string;
  readonly location?: string | null;
  readonly durationHours: number;
}

function formatTime(isoTime: string): string {
  try {
    const date = new Date(isoTime);
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  } catch {
    return isoTime;
  }
}

function formatDuration(hours: number): string {
  if (hours < 1) {
    return `${Math.round(hours * 60)}min`;
  }
  const wholeHours = Math.floor(hours);
  const minutes = Math.round((hours - wholeHours) * 60);
  if (minutes === 0) {
    return `${wholeHours}h`;
  }
  return `${wholeHours}h${minutes}min`;
}

export function MeetingCard({
  title,
  startTime,
  endTime,
  location,
  durationHours,
}: MeetingCardProps) {
  return (
    <div className="bg-white rounded-lg border border-gray-200 p-3 hover:shadow-sm transition-shadow">
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            <svg
              className="w-4 h-4 text-blue-500 flex-shrink-0"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={1.5}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            <h4 className="text-sm font-medium text-gray-900 truncate">{title}</h4>
          </div>

          <div className="flex items-center gap-3 text-xs text-gray-500">
            <span>
              {formatTime(startTime)} - {formatTime(endTime)}
            </span>
            {location && (
              <span className="flex items-center gap-1 truncate">
                <svg
                  className="w-3 h-3 flex-shrink-0"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={1.5}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M15 10.5a3 3 0 11-6 0 3 3 0 016 0z"
                  />
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M19.5 10.5c0 7.142-7.5 11.25-7.5 11.25S4.5 17.642 4.5 10.5a7.5 7.5 0 1115 0z"
                  />
                </svg>
                <span className="truncate">{location}</span>
              </span>
            )}
          </div>
        </div>

        <span className="inline-flex px-2 py-0.5 rounded text-xs font-medium bg-blue-100 text-blue-700 flex-shrink-0">
          {formatDuration(durationHours)}
        </span>
      </div>
    </div>
  );
}
