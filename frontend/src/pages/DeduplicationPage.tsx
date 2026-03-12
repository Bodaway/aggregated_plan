import { useState } from 'react';
import { useDedup } from '@/hooks/use-dedup';
import { SuggestionCard } from '@/components/dedup/SuggestionCard';

export function DeduplicationPage() {
  const { suggestions, loading, error, confirmDeduplication } = useDedup();
  const [processingId, setProcessingId] = useState<string | null>(null);

  const handleAccept = async (suggestionId: string) => {
    setProcessingId(suggestionId);
    try {
      await confirmDeduplication(suggestionId, true);
    } finally {
      setProcessingId(null);
    }
  };

  const handleReject = async (suggestionId: string) => {
    setProcessingId(suggestionId);
    try {
      await confirmDeduplication(suggestionId, false);
    } finally {
      setProcessingId(null);
    }
  };

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load deduplication suggestions</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  if (loading && suggestions.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
          <p className="text-gray-500 text-sm">Scanning for duplicates...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4 max-w-4xl">
      {/* Header with count */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <svg
            className="w-5 h-5 text-gray-500"
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
          <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">
            Deduplication Suggestions
          </h2>
          <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-gray-100 text-gray-600">
            {suggestions.length} suggestion{suggestions.length !== 1 ? 's' : ''}
          </span>
        </div>
      </div>

      {/* Empty state */}
      {suggestions.length === 0 && (
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
              d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <p className="text-gray-500 text-sm font-medium">No duplicate suggestions</p>
          <p className="text-gray-400 text-xs mt-1">
            All tasks appear to be unique. Check back after syncing new data.
          </p>
        </div>
      )}

      {/* Suggestion list */}
      <div className="space-y-3">
        {suggestions.map(suggestion => (
          <SuggestionCard
            key={suggestion.id}
            id={suggestion.id}
            taskA={{
              title: suggestion.taskA.title,
              source: suggestion.taskA.source,
              assignee: suggestion.taskA.assignee,
              project: suggestion.taskA.project?.name,
            }}
            taskB={{
              title: suggestion.taskB.title,
              source: suggestion.taskB.source,
              assignee: suggestion.taskB.assignee,
              project: suggestion.taskB.project?.name,
            }}
            confidenceScore={suggestion.confidenceScore}
            titleSimilarity={suggestion.titleSimilarity}
            assigneeMatch={suggestion.assigneeMatch}
            projectMatch={suggestion.projectMatch}
            onAccept={() => handleAccept(suggestion.id)}
            onReject={() => handleReject(suggestion.id)}
            processing={processingId === suggestion.id}
          />
        ))}
      </div>
    </div>
  );
}
