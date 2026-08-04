import { useEffect, useRef, useState } from 'react';

import { ProjectSummarySidebar } from '@/components/timesheet/ProjectSummarySidebar';
import { TimesheetTimeline } from '@/components/timesheet/TimesheetTimeline';
import { formatDate, formatDisplayDateFr, getNextDay, getPrevDay } from '@/lib/date-utils';
import { useGryzzlyProjects, useTimesheet, type ReconstructResult } from '@/hooks/use-timesheet';

export function TimesheetPage() {
  const [date, setDate] = useState<Date>(new Date());
  const { day, loading, error, reconstruct, saveLines, validate, markOff, refetch } = useTimesheet(date);
  const { projects } = useGryzzlyProjects();

  const [refreshMsg, setRefreshMsg] = useState<ReconstructResult | null>(null);
  const [confirmRefresh, setConfirmRefresh] = useState(false);

  // Tracks the currently selected day so a reconstruct resolving after the user
  // navigated away doesn't paint a stale message onto the new day.
  const selectedDateRef = useRef(formatDate(date));
  useEffect(() => {
    selectedDateRef.current = formatDate(date);
  }, [date]);

  // Reset transient refresh UI whenever the day changes.
  useEffect(() => {
    setRefreshMsg(null);
    setConfirmRefresh(false);
  }, [date]);

  const onRefresh = () => setConfirmRefresh(true);

  const confirmReconstruct = async () => {
    setConfirmRefresh(false);
    const requestedDate = formatDate(date);
    const r = await reconstruct();
    if (selectedDateRef.current === requestedDate) setRefreshMsg(r);
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <button onClick={() => setDate((d) => getPrevDay(d))} className="px-2 py-1 text-sm rounded border border-gray-300 hover:bg-gray-50">←</button>
        <button onClick={() => setDate(new Date())} className="px-2 py-1 text-sm rounded border border-gray-300 hover:bg-gray-50">Aujourd'hui</button>
        <button onClick={() => setDate((d) => getNextDay(d))} className="px-2 py-1 text-sm rounded border border-gray-300 hover:bg-gray-50">→</button>
        <span className="ml-2 text-sm font-medium text-gray-700">{formatDisplayDateFr(date)}</span>
        <button onClick={refetch} className="ml-auto px-2 py-1 text-xs text-gray-500 hover:text-gray-800">⟳</button>
      </div>

      {confirmRefresh && (
        <div className="rounded border border-amber-300 bg-amber-50 p-3 text-sm text-amber-800 space-y-2">
          <p>Reconstruire depuis les signaux ? Cela écrase les modifications non enregistrées de ce jour.</p>
          <div className="flex gap-2">
            <button onClick={confirmReconstruct} className="bg-amber-600 text-white text-sm rounded px-3 py-1 hover:bg-amber-700">Confirmer</button>
            <button onClick={() => setConfirmRefresh(false)} className="bg-white border border-gray-300 text-gray-700 text-sm rounded px-3 py-1 hover:bg-gray-50">Annuler</button>
          </div>
        </div>
      )}

      {refreshMsg && (
        <div className={`text-sm ${refreshMsg.isError ? 'text-red-600' : 'text-green-700'}`}>{refreshMsg.message}</div>
      )}

      {error && <div className="text-sm text-red-600">Échec du chargement de la feuille de temps : {error.message}</div>}
      {loading && !day && <div className="text-sm text-gray-400">Reconstruction…</div>}

      {day && (
        <div className="flex gap-6 items-start">
          <div className="flex-1 bg-white rounded-lg border border-gray-200 p-4">
            <TimesheetTimeline blocks={day.blocks} />
            {day.unresolved.length > 0 && (
              <div className="mt-3 text-xs text-amber-700">
                {day.unresolved.length} signal(aux) non résolu(s) — attribuez les heures à un projet dans le panneau latéral.
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
