import { useState, useCallback } from 'react';
import { useActivity } from '@/hooks/use-activity';
import { ActivityTimer } from '@/components/activity/ActivityTimer';
import { ActivityTimeline } from '@/components/activity/ActivityTimeline';
import { SlotCard } from '@/components/activity/SlotCard';
import { formatDate, formatDisplayDate, getNextDay, getPrevDay } from '@/lib/date-utils';

export function ActivityJournalPage() {
  const [currentDate, setCurrentDate] = useState(() => new Date());
  const dateStr = formatDate(currentDate);

  const { slots, currentActivity, loading, error, startActivity, stopActivity, deleteSlot } =
    useActivity(dateStr);

  const goToPreviousDay = useCallback(() => {
    setCurrentDate(prev => getPrevDay(prev));
  }, []);

  const goToNextDay = useCallback(() => {
    setCurrentDate(prev => getNextDay(prev));
  }, []);

  const goToToday = useCallback(() => {
    setCurrentDate(new Date());
  }, []);

  const handleStart = useCallback(
    (taskId?: string) => {
      startActivity(taskId);
    },
    [startActivity]
  );

  const handleStop = useCallback(() => {
    stopActivity();
  }, [stopActivity]);

  const handleDelete = useCallback(
    (id: string) => {
      deleteSlot(id);
    },
    [deleteSlot]
  );

  // Separate completed slots (with endTime) from in-progress
  const completedSlots = slots.filter(s => s.endTime !== null);

  // Compute total duration
  const totalMinutes = completedSlots.reduce(
    (sum, s) => sum + (s.durationMinutes ?? 0),
    0
  );
  const totalHours = Math.floor(totalMinutes / 60);
  const totalMins = totalMinutes % 60;

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load activity journal</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4 max-w-4xl">
      {/* Timer section */}
      <ActivityTimer
        currentActivity={currentActivity}
        onStart={handleStart}
        onStop={handleStop}
      />

      {/* Date navigation */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <button
            onClick={goToPreviousDay}
            className="p-1.5 rounded-md border border-gray-300 hover:bg-gray-50 transition-colors"
            aria-label="Previous day"
          >
            <svg
              className="w-4 h-4 text-gray-600"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 19.5L8.25 12l7.5-7.5" />
            </svg>
          </button>
          <h2 className="text-lg font-semibold text-gray-800">
            {formatDisplayDate(currentDate)}
          </h2>
          <button
            onClick={goToNextDay}
            className="p-1.5 rounded-md border border-gray-300 hover:bg-gray-50 transition-colors"
            aria-label="Next day"
          >
            <svg
              className="w-4 h-4 text-gray-600"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
            </svg>
          </button>
          <button
            onClick={goToToday}
            className="px-3 py-1 text-xs font-medium text-blue-600 border border-blue-300 rounded-md hover:bg-blue-50 transition-colors"
          >
            Today
          </button>
        </div>

        {/* Day summary */}
        <div className="flex items-center gap-4">
          <div className="text-right">
            <p className="text-xs text-gray-500">Activities</p>
            <p className="text-sm font-medium text-gray-800">{completedSlots.length}</p>
          </div>
          <div className="text-right">
            <p className="text-xs text-gray-500">Total Time</p>
            <p className="text-sm font-medium text-gray-800">
              {totalHours > 0 ? `${totalHours}h ` : ''}{totalMins}min
            </p>
          </div>
        </div>
      </div>

      {loading && slots.length === 0 ? (
        <div className="flex items-center justify-center h-48">
          <div className="text-center">
            <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
            <p className="text-gray-500 text-sm">Loading activity journal...</p>
          </div>
        </div>
      ) : (
        <>
          {/* Timeline visualization */}
          <ActivityTimeline slots={slots} />

          {/* Completed slots list */}
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">
                Activity Log
              </h3>
              <span className="text-xs text-gray-400">
                {completedSlots.length} entr{completedSlots.length !== 1 ? 'ies' : 'y'}
              </span>
            </div>

            {completedSlots.length === 0 ? (
              <div className="text-center py-8">
                <svg
                  className="w-10 h-10 text-gray-300 mx-auto mb-2"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={1}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                <p className="text-gray-500 text-sm">No completed activities for this day</p>
                <p className="text-gray-400 text-xs mt-1">
                  Start the timer above to begin tracking
                </p>
              </div>
            ) : (
              <div className="space-y-2">
                {completedSlots.map(slot => (
                  <SlotCard
                    key={slot.id}
                    id={slot.id}
                    taskTitle={slot.task?.title ?? null}
                    startTime={slot.startTime}
                    endTime={slot.endTime}
                    halfDay={slot.halfDay}
                    durationMinutes={slot.durationMinutes}
                    onDelete={handleDelete}
                  />
                ))}
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}
