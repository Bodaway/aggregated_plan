import { SOURCE_COLORS } from '@/lib/constants';

interface TaskSummary {
  readonly title: string;
  readonly source: string;
  readonly assignee?: string | null;
  readonly project?: string | null;
}

interface SuggestionCardProps {
  readonly id: string;
  readonly taskA: TaskSummary;
  readonly taskB: TaskSummary;
  readonly confidenceScore: number;
  readonly titleSimilarity: number;
  readonly assigneeMatch: boolean;
  readonly projectMatch: boolean;
  readonly onAccept: () => void;
  readonly onReject: () => void;
  readonly processing?: boolean;
}

/** Returns a color class for the confidence score bar. */
function getConfidenceColor(score: number): string {
  if (score >= 0.8) return 'bg-red-500';
  if (score >= 0.6) return 'bg-yellow-500';
  return 'bg-blue-500';
}

/** Returns a text color class for the confidence label. */
function getConfidenceTextColor(score: number): string {
  if (score >= 0.8) return 'text-red-700';
  if (score >= 0.6) return 'text-yellow-700';
  return 'text-blue-700';
}

/** Displays a single task's summary information. */
function TaskSide({ task, label }: { readonly task: TaskSummary; readonly label: string }) {
  const sourceColor =
    (SOURCE_COLORS as Record<string, string>)[task.source] ?? '#6B7280';

  return (
    <div className="flex-1 min-w-0">
      <div className="flex items-center gap-2 mb-1">
        <span className="text-xs font-medium text-gray-400 uppercase">{label}</span>
        <span
          className="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium text-white"
          style={{ backgroundColor: sourceColor }}
        >
          {task.source}
        </span>
      </div>
      <p className="text-sm font-medium text-gray-800 truncate" title={task.title}>
        {task.title}
      </p>
      <div className="mt-1 space-y-0.5">
        {task.assignee && (
          <div className="flex items-center gap-1 text-xs text-gray-500">
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
                d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z"
              />
            </svg>
            <span className="truncate">{task.assignee}</span>
          </div>
        )}
        {task.project && (
          <div className="flex items-center gap-1 text-xs text-gray-500">
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
                d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z"
              />
            </svg>
            <span className="truncate">{task.project}</span>
          </div>
        )}
      </div>
    </div>
  );
}

/** Match indicator pill. */
function MatchIndicator({
  label,
  matched,
}: {
  readonly label: string;
  readonly matched: boolean;
}) {
  return (
    <span
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${
        matched ? 'bg-green-100 text-green-700' : 'bg-gray-100 text-gray-500'
      }`}
    >
      {matched ? (
        <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12.75l6 6 9-13.5" />
        </svg>
      ) : (
        <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      )}
      {label}
    </span>
  );
}

export function SuggestionCard({
  taskA,
  taskB,
  confidenceScore,
  titleSimilarity,
  assigneeMatch,
  projectMatch,
  onAccept,
  onReject,
  processing = false,
}: SuggestionCardProps) {
  const percentage = Math.round(confidenceScore * 100);
  const titlePct = Math.round(titleSimilarity * 100);

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4 hover:shadow-sm transition-shadow">
      {/* Confidence score header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <svg
            className="w-4 h-4 text-gray-400"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M7.5 21L3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5"
            />
          </svg>
          <span className="text-xs font-medium text-gray-500 uppercase tracking-wider">
            Potential Duplicate
          </span>
        </div>
        <span className={`text-sm font-bold ${getConfidenceTextColor(confidenceScore)}`}>
          {percentage}% match
        </span>
      </div>

      {/* Confidence bar */}
      <div className="w-full bg-gray-100 rounded-full h-1.5 mb-4">
        <div
          className={`h-1.5 rounded-full transition-all ${getConfidenceColor(confidenceScore)}`}
          style={{ width: `${percentage}%` }}
        />
      </div>

      {/* Side-by-side task comparison */}
      <div className="flex gap-4 mb-3">
        <TaskSide task={taskA} label="Task A" />
        <div className="flex items-center">
          <div className="w-px h-full bg-gray-200" />
        </div>
        <TaskSide task={taskB} label="Task B" />
      </div>

      {/* Match indicators */}
      <div className="flex items-center gap-2 mb-4">
        <span className="text-xs text-gray-400 mr-1">Signals:</span>
        <MatchIndicator label={`Title ${titlePct}%`} matched={titleSimilarity >= 0.5} />
        <MatchIndicator label="Assignee" matched={assigneeMatch} />
        <MatchIndicator label="Project" matched={projectMatch} />
      </div>

      {/* Action buttons */}
      <div className="flex items-center gap-2 border-t border-gray-100 pt-3">
        <button
          type="button"
          onClick={onAccept}
          disabled={processing}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-white bg-green-600 rounded-md hover:bg-green-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          {processing ? (
            <div className="w-3.5 h-3.5 border-2 border-white border-t-transparent rounded-full animate-spin" />
          ) : (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12.75l6 6 9-13.5" />
            </svg>
          )}
          Merge
        </button>
        <button
          type="button"
          onClick={onReject}
          disabled={processing}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-gray-600 bg-white border border-gray-300 rounded-md hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
          Not a Duplicate
        </button>
      </div>
    </div>
  );
}
