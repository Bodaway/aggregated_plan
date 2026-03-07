import { useState } from 'react';
import { useWorkload } from '@/hooks/use-workload';
import { WorkloadChart } from '@/components/workload/WorkloadChart';
import { formatDate, getWeekStart } from '@/lib/date-utils';
import { addDays, format } from 'date-fns';
import { HALF_DAY_HOURS } from '@/lib/constants';
import type { WorkloadHalfDay } from '@/hooks/use-workload';

const WEEKDAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday'] as const;

/** Groups half-day slots by date for the grid display. */
function groupByDate(
  halfDays: readonly WorkloadHalfDay[]
): ReadonlyMap<string, { am: WorkloadHalfDay | null; pm: WorkloadHalfDay | null }> {
  const map = new Map<string, { am: WorkloadHalfDay | null; pm: WorkloadHalfDay | null }>();

  halfDays.forEach(slot => {
    const existing = map.get(slot.date) ?? { am: null, pm: null };
    if (slot.halfDay === 'AM') {
      map.set(slot.date, { ...existing, am: slot });
    } else {
      map.set(slot.date, { ...existing, pm: slot });
    }
  });

  return map;
}

function getConsumptionColor(consumption: number): string {
  const ratio = consumption / HALF_DAY_HOURS;
  if (ratio >= 1) return 'bg-red-100 border-red-200';
  if (ratio >= 0.75) return 'bg-yellow-100 border-yellow-200';
  if (ratio > 0) return 'bg-blue-50 border-blue-200';
  return 'bg-green-50 border-green-200';
}

export function WorkloadPage() {
  const [weekOffset, setWeekOffset] = useState(0);
  const baseWeekStart = getWeekStart(new Date());
  const currentWeekStart = addDays(baseWeekStart, weekOffset * 7);
  const weekStartStr = formatDate(currentWeekStart);
  const weekEndDate = addDays(currentWeekStart, 4); // Friday

  const { data, loading, error } = useWorkload(weekStartStr);

  const goToPreviousWeek = () => setWeekOffset(prev => prev - 1);
  const goToNextWeek = () => setWeekOffset(prev => prev + 1);
  const goToCurrentWeek = () => setWeekOffset(0);

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load workload data</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  const grouped = data ? groupByDate(data.halfDays) : new Map();

  return (
    <div className="space-y-4">
      {/* Week navigation */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <button
            onClick={goToPreviousWeek}
            className="p-1.5 rounded-md border border-gray-300 hover:bg-gray-50 transition-colors"
            aria-label="Previous week"
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
            {format(currentWeekStart, 'MMM d')} - {format(weekEndDate, 'MMM d, yyyy')}
          </h2>
          <button
            onClick={goToNextWeek}
            className="p-1.5 rounded-md border border-gray-300 hover:bg-gray-50 transition-colors"
            aria-label="Next week"
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
            onClick={goToCurrentWeek}
            className="px-3 py-1 text-xs font-medium text-blue-600 border border-blue-300 rounded-md hover:bg-blue-50 transition-colors"
          >
            This Week
          </button>
        </div>

        {/* Summary stats */}
        {data && (
          <div className="flex items-center gap-4">
            <div className="text-right">
              <p className="text-xs text-gray-500">Capacity</p>
              <p className="text-sm font-medium text-gray-800">
                {data.capacity}h
              </p>
            </div>
            <div className="text-right">
              <p className="text-xs text-gray-500">Meetings</p>
              <p className="text-sm font-medium text-blue-600">
                {data.totalMeetings}h
              </p>
            </div>
            <div className="text-right">
              <p className="text-xs text-gray-500">Planned</p>
              <p className="text-sm font-medium text-gray-800">
                {data.totalPlanned}h
              </p>
            </div>
            {data.overload && (
              <span className="inline-flex items-center gap-1 px-2 py-1 rounded-full text-xs font-medium bg-red-100 text-red-700">
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
                Overloaded
              </span>
            )}
          </div>
        )}
      </div>

      {loading && !data ? (
        <div className="flex items-center justify-center h-64">
          <div className="text-center">
            <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
            <p className="text-gray-500 text-sm">Loading workload...</p>
          </div>
        </div>
      ) : (
        <>
          {/* Workload chart */}
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider mb-3">
              Weekly Overview
            </h3>
            {data ? (
              <WorkloadChart halfDays={data.halfDays} />
            ) : (
              <p className="text-gray-500 text-sm text-center py-8">
                No workload data available for this week
              </p>
            )}
          </div>

          {/* Half-day detail grid */}
          <div className="bg-white rounded-lg border border-gray-200 p-4">
            <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider mb-3">
              Half-Day Details
            </h3>

            {data && data.halfDays.length > 0 ? (
              <div className="grid grid-cols-5 gap-3">
                {WEEKDAYS.map((dayName, index) => {
                  const dayDate = formatDate(addDays(currentWeekStart, index));
                  const slots = grouped.get(dayDate);

                  return (
                    <div key={dayName}>
                      <h4 className="text-xs font-medium text-gray-600 mb-2 text-center">
                        {dayName}
                        <br />
                        <span className="text-gray-400">{format(addDays(currentWeekStart, index), 'MMM d')}</span>
                      </h4>

                      {/* AM slot */}
                      <HalfDaySlotCard label="AM" slot={slots?.am ?? null} />

                      {/* PM slot */}
                      <HalfDaySlotCard label="PM" slot={slots?.pm ?? null} />
                    </div>
                  );
                })}
              </div>
            ) : (
              <p className="text-gray-500 text-sm text-center py-4">
                No detail data available
              </p>
            )}
          </div>
        </>
      )}
    </div>
  );
}

interface HalfDaySlotCardProps {
  readonly label: string;
  readonly slot: WorkloadHalfDay | null;
}

function HalfDaySlotCard({ label, slot }: HalfDaySlotCardProps) {
  if (!slot) {
    return (
      <div className="mb-2 p-2 rounded border border-gray-100 bg-gray-50 text-center">
        <span className="text-xs text-gray-400">{label}</span>
        <p className="text-xs text-gray-300 mt-1">No data</p>
      </div>
    );
  }

  const colorClass = getConsumptionColor(slot.consumption);

  return (
    <div className={`mb-2 p-2 rounded border ${colorClass}`}>
      <div className="flex items-center justify-between mb-1">
        <span className="text-xs font-medium text-gray-600">{label}</span>
        <span className="text-xs text-gray-500">
          {slot.consumption.toFixed(1)}h / {HALF_DAY_HOURS}h
        </span>
      </div>

      {/* Meetings in this slot */}
      {slot.meetings.length > 0 && (
        <div className="space-y-0.5 mt-1">
          {slot.meetings.map((meeting, idx) => (
            <div
              key={`meeting-${idx}`}
              className="flex items-center gap-1 text-xs text-blue-600"
            >
              <svg
                className="w-2.5 h-2.5 flex-shrink-0"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
              <span className="truncate">{meeting.title}</span>
            </div>
          ))}
        </div>
      )}

      {/* Tasks in this slot */}
      {slot.tasks.length > 0 && (
        <div className="space-y-0.5 mt-1">
          {slot.tasks.map((task, idx) => (
            <div
              key={`task-${idx}`}
              className="flex items-center gap-1 text-xs text-gray-600"
            >
              <svg
                className="w-2.5 h-2.5 flex-shrink-0"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
              <span className="truncate">{task.title}</span>
            </div>
          ))}
        </div>
      )}

      {slot.isFree && slot.meetings.length === 0 && slot.tasks.length === 0 && (
        <p className="text-xs text-green-600 mt-1">Free</p>
      )}
    </div>
  );
}
