import { useState } from 'react';

import { ProjectSummarySidebar } from '@/components/timesheet/ProjectSummarySidebar';
import { TimesheetTimeline } from '@/components/timesheet/TimesheetTimeline';
import { formatDisplayDate, getNextDay, getPrevDay } from '@/lib/date-utils';
import { useGryzzlyProjects, useTimesheet } from '@/hooks/use-timesheet';

export function TimesheetPage() {
  const [date, setDate] = useState<Date>(new Date());
  const { day, loading, error, reconstruct, saveLines, validate, markOff, refetch } = useTimesheet(date);
  const { projects } = useGryzzlyProjects();

  const onRefresh = () => {
    if (window.confirm('Reconstruct from signals? This overwrites unsaved manual edits for this day.')) {
      void reconstruct();
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <button onClick={() => setDate((d) => getPrevDay(d))} className="px-2 py-1 text-sm rounded border border-gray-300 hover:bg-gray-50">←</button>
        <button onClick={() => setDate(new Date())} className="px-2 py-1 text-sm rounded border border-gray-300 hover:bg-gray-50">Today</button>
        <button onClick={() => setDate((d) => getNextDay(d))} className="px-2 py-1 text-sm rounded border border-gray-300 hover:bg-gray-50">→</button>
        <span className="ml-2 text-sm font-medium text-gray-700">{formatDisplayDate(date)}</span>
        <button onClick={refetch} className="ml-auto px-2 py-1 text-xs text-gray-500 hover:text-gray-800">⟳</button>
      </div>

      {error && <div className="text-sm text-red-600">Failed to load timesheet: {error.message}</div>}
      {loading && !day && <div className="text-sm text-gray-400">Reconstructing…</div>}

      {day && (
        <div className="flex gap-6 items-start">
          <div className="flex-1 bg-white rounded-lg border border-gray-200 p-4">
            <TimesheetTimeline blocks={day.blocks} />
            {day.unresolved.length > 0 && (
              <div className="mt-3 text-xs text-amber-700">
                {day.unresolved.length} unresolved signal(s) — assign hours to a project in the sidebar.
              </div>
            )}
          </div>
          <ProjectSummarySidebar
            day={day}
            projects={projects}
            onSaveLines={saveLines}
            onValidate={validate}
            onMarkOff={markOff}
            onRefresh={onRefresh}
            busy={loading}
          />
        </div>
      )}
    </div>
  );
}
