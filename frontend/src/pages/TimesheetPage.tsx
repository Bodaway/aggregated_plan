import { useEffect, useRef, useState } from 'react';

import { ProjectSummarySidebar } from '@/components/timesheet/ProjectSummarySidebar';
import { QuarterEditor } from '@/components/timesheet/QuarterEditor';
import { TimesheetLanes } from '@/components/timesheet/TimesheetLanes';
import { formatDate, formatDisplayDateFr, getNextDay, getPrevDay } from '@/lib/date-utils';
import { useGryzzlyProjects, useTimesheet, type ReconstructResult } from '@/hooks/use-timesheet';

/** `HH:MM` from a bare local NaiveDateTime (the wire format the backend emits). */
function formatLocalHm(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '--:--';
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

/** `1 h 34` from a minute count. */
function formatDuration(minutes: number): string {
  return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, '0')}`;
}

export function TimesheetPage() {
  const [date, setDate] = useState<Date>(new Date());
  const {
    day, loading, error, reconstruct, assignLaneGryzzlyTask, setShare, clearShare,
    resetQuarter, validate, markOff, refetch,
  } = useTimesheet(date);
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

  // Reassigning from a lane row rebuilds the day on the spot — no confirmation, since
  // the user just asked for exactly that change and pinned shares survive a rebuild.
  // Guarded by the same stale-day check as a manual reconstruct.
  const onAssignLaneTask = async (taskId: string, gryzzlyTaskId: string | null) => {
    const requestedDate = formatDate(date);
    const r = await assignLaneGryzzlyTask(taskId, gryzzlyTaskId);
    if (selectedDateRef.current === requestedDate) setRefreshMsg(r);
  };

  const confirmReconstruct = async () => {
    setConfirmRefresh(false);
    const requestedDate = formatDate(date);
    const r = await reconstruct();
    if (selectedDateRef.current === requestedDate) setRefreshMsg(r);
  };

  // A validated or submitted day is history: nothing on it is editable any more.
  const readOnly = day?.status === 'VALIDATED' || day?.status === 'SUBMITTED';

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
          <div className="flex-1 space-y-4">
            <div className="rounded-lg border border-gray-200 bg-white p-4">
              <h2 className="mb-3 text-sm font-semibold uppercase tracking-wider text-gray-700">
                Travail concurrent
              </h2>
              <TimesheetLanes
                lanes={day.lanes}
                quarters={day.quarters}
                projects={projects}
                readOnly={readOnly}
                onAssignLaneTask={(taskId, gryzzlyTaskId) => void onAssignLaneTask(taskId, gryzzlyTaskId)}
              />
              {day.outsideWorkday.length > 0 && (
                <p className="mt-3 text-xs text-amber-700">
                  ⚠{' '}
                  {formatDuration(day.outsideWorkday.reduce((s, o) => s + o.minutes, 0))} de traces
                  hors plage horaire —{' '}
                  {day.outsideWorkday.map((o) => o.label).join(', ')}. Élargissez la journée dans
                  les réglages pour les déclarer.
                </p>
              )}
            </div>

            <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
              {day.quarters.map((q) => (
                <QuarterEditor
                  key={q.index}
                  quarter={q}
                  projects={projects}
                  roundingIncrement={day.roundingIncrement}
                  readOnly={readOnly}
                  onSetShare={setShare}
                  onClearShare={clearShare}
                  onReset={resetQuarter}
                />
              ))}
            </div>

            {day.unresolved.length > 0 && (
              <div className="mt-3 text-xs text-amber-700">
                <p>
                  {day.unresolved.length} signal(aux) non résolu(s) — attribuez les heures à un projet dans le panneau latéral.
                </p>
                <ul className="mt-1 space-y-0.5">
                  {day.unresolved.map((u) => (
                    <li key={u.sourceRef} className="truncate">
                      <span className="font-mono text-amber-800">{formatLocalHm(u.at)}</span> {u.label}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
          <ProjectSummarySidebar
            day={day}
            projects={projects}
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
