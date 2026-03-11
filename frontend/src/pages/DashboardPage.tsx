import { useState, useCallback } from 'react';
import { useDashboard } from '@/hooks/use-dashboard';
import { TaskList } from '@/components/task/TaskList';
import { MeetingList } from '@/components/meeting/MeetingList';
import { WorkloadChart } from '@/components/workload/WorkloadChart';
import { AlertPanel } from '@/components/alert/AlertPanel';
import { SyncStatusBar } from '@/components/sync/SyncStatusBar';
import { formatDate, formatDisplayDate, getNextDay, getPrevDay } from '@/lib/date-utils';
import type { TaskCardProps } from '@/components/task/TaskCard';
import { TaskEditSheet } from '@/components/task/TaskEditSheet';

/** Priority order for sorting tasks: urgentImportant first, neither last. */
const QUADRANT_PRIORITY: Record<string, number> = {
  UrgentImportant: 0,
  Important: 1,
  Urgent: 2,
  Neither: 3,
};

function getQuadrantPriority(quadrant: string): number {
  return QUADRANT_PRIORITY[quadrant] ?? 4;
}

export function DashboardPage() {
  const [editingTaskId, setEditingTaskId] = useState<string | null>(null);

  const handleTaskClick = useCallback((taskId: string) => {
    setEditingTaskId(taskId);
  }, []);

  const handleSheetClose = useCallback(() => {
    setEditingTaskId(null);
  }, []);

  const [currentDate, setCurrentDate] = useState(() => new Date());
  const dateStr = formatDate(currentDate);
  const { data, loading, error } = useDashboard(dateStr);

  const goToPreviousDay = () => {
    setCurrentDate(prev => getPrevDay(prev));
  };

  const goToNextDay = () => {
    setCurrentDate(prev => getNextDay(prev));
  };

  const goToToday = () => {
    setCurrentDate(new Date());
  };

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load dashboard</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  // Map dashboard tasks to TaskCardProps
  const taskCards: readonly TaskCardProps[] = data
    ? [...data.tasks]
        .sort((a, b) => getQuadrantPriority(a.quadrant) - getQuadrantPriority(b.quadrant))
        .map(t => ({
          id: t.id,
          title: t.title,
          source: t.source ?? 'PERSONAL',
          sourceId: t.sourceId ?? null,
          status: t.status,
          jiraStatus: t.jiraStatus ?? null,
          urgency: t.urgency,
          impact: t.impact,
          quadrant: t.quadrant,
          deadline: t.deadline,
          assignee: t.assignee,
          projectName: t.project?.name ?? null,
          tags: t.tags,
          effectiveRemainingHours: t.effectiveRemainingHours ?? null,
          effectiveEstimatedHours: t.effectiveEstimatedHours ?? null,
          jiraTimeSpentSeconds: t.jiraTimeSpentSeconds ?? null,
          onClick: () => handleTaskClick(t.id),
        }))
    : [];

  return (
    <div className="space-y-4">
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
      </div>

      {/* Top: Sync status */}
      {data && <SyncStatusBar statuses={data.syncStatuses} />}

      {loading && !data ? (
        <div className="flex items-center justify-center h-64">
          <div className="text-center">
            <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
            <p className="text-gray-500 text-sm">Loading dashboard...</p>
          </div>
        </div>
      ) : (
        <>
          {/* Main grid: Tasks + Meetings/Workload */}
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
            {/* Left column: Tasks of the day (2/3 width) */}
            <div className="lg:col-span-2">
              <div className="bg-white rounded-lg border border-gray-200 p-4">
                <div className="flex items-center justify-between mb-3">
                  <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">
                    Followed Tasks
                  </h3>
                  <span className="text-xs text-gray-400">
                    {taskCards.length} task{taskCards.length !== 1 ? 's' : ''}
                  </span>
                </div>
                <TaskList tasks={taskCards} emptyMessage="No tasks scheduled for this day" />
              </div>
            </div>

            {/* Right column: Meetings + Workload (1/3 width) */}
            <div className="space-y-4">
              {/* Meetings */}
              <div className="bg-white rounded-lg border border-gray-200 p-4">
                <div className="flex items-center justify-between mb-3">
                  <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">
                    Meetings
                  </h3>
                  <span className="text-xs text-gray-400">
                    {data?.meetings.length ?? 0} meeting
                    {(data?.meetings.length ?? 0) !== 1 ? 's' : ''}
                  </span>
                </div>
                <MeetingList
                  meetings={data?.meetings ?? []}
                  emptyMessage="No meetings today"
                />
              </div>

              {/* Weekly workload mini chart */}
              <div className="bg-white rounded-lg border border-gray-200 p-4">
                <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider mb-3">
                  Weekly Workload
                </h3>
                {data?.weeklyWorkload ? (
                  <>
                    <WorkloadChart halfDays={data.weeklyWorkload.halfDays} compact />
                    {data.weeklyWorkload.overload && (
                      <div className="mt-2 flex items-center gap-1 text-xs text-red-600">
                        <svg
                          className="w-3.5 h-3.5"
                          fill="none"
                          viewBox="0 0 24 24"
                          stroke="currentColor"
                          strokeWidth={1.5}
                        >
                          <path
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
                          />
                        </svg>
                        <span>Overloaded this week</span>
                      </div>
                    )}
                  </>
                ) : (
                  <p className="text-gray-500 text-sm text-center py-4">
                    No workload data available
                  </p>
                )}
              </div>
            </div>
          </div>

          {/* Bottom: Alerts */}
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider mb-3">
              Alerts
            </h3>
            <AlertPanel alerts={data?.alerts ?? []} />
          </div>
        </>
      )}
      <TaskEditSheet taskId={editingTaskId} onClose={handleSheetClose} />
    </div>
  );
}
