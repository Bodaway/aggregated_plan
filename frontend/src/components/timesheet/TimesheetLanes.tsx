import { LaneGryzzlyPicker } from './LaneGryzzlyPicker';
import type { Lane, Quarter } from '@/hooks/use-timesheet';
import type { ProjectOption } from '@/hooks/use-timesheet';

/** `HH:MM` from local minutes-from-midnight. */
function hm(min: number): string {
  return `${String(Math.floor(min / 60)).padStart(2, '0')}:${String(min % 60).padStart(2, '0')}`;
}

interface Props {
  lanes: Lane[];
  quarters: Quarter[];
  projects: ProjectOption[];
  /** Reassign the Gryzzly task of the plan task behind a lane. Omitted — or a
   *  `readOnly` day — leaves the project as plain text. */
  onAssignLaneTask?: (taskId: string, gryzzlyTaskId: string | null) => void;
  readOnly?: boolean;
}

/**
 * The concurrent view: one row per task, bars where it can be shown to have been alive.
 *
 * Rows overlap on purpose — two sessions running at once produce two bars over the same
 * minutes. That overlap is the thing the previous single-track timeline could not show,
 * and the reason a whole afternoon used to be credited to whichever task logged first.
 */
export function TimesheetLanes({ lanes, quarters, projects, onAssignLaneTask, readOnly }: Props) {
  if (lanes.length === 0) {
    return (
      <p className="text-sm text-gray-400">
        Aucune trace pour cette journée — reconstruisez pour voir les voies.
      </p>
    );
  }

  // The drawing window spans every quarter; without quarters, fall back to the lanes.
  const startMin = quarters.length
    ? Math.min(...quarters.map((q) => q.startMin))
    : Math.min(...lanes.flatMap((l) => l.intervals.map((i) => i.startMin)));
  const endMin = quarters.length
    ? Math.max(...quarters.map((q) => q.endMin))
    : Math.max(...lanes.flatMap((l) => l.intervals.map((i) => i.endMin)));
  const span = Math.max(1, endMin - startMin);
  const pct = (min: number) => ((min - startMin) / span) * 100;

  const projectName = (id: string | null) =>
    id ? (projects.find((p) => p.id === id)?.label ?? id) : 'sans projet Gryzzly';

  return (
    <div className="space-y-1">
      <div className="relative h-4 ml-48 text-[10px] text-gray-400">
        {quarters.map((q) => (
          <span key={q.index} className="absolute" style={{ left: `${pct(q.startMin)}%` }}>
            {hm(q.startMin)}
          </span>
        ))}
        <span className="absolute right-0">{hm(endMin)}</span>
      </div>

      {lanes.map((lane) => {
        const { taskId } = lane;
        // Bound in the narrowed branch so the callback needs no assertion: a lane
        // without a task id is a meeting or an unmatched repo, with nothing to reassign.
        const projectCell =
          taskId !== null && onAssignLaneTask && !readOnly ? (
            <LaneGryzzlyPicker
              laneLabel={lane.label}
              projectLabel={projectName(lane.gryzzlyProjectId)}
              hasProject={lane.gryzzlyProjectId !== null}
              onAssign={(gryzzlyTaskId) => onAssignLaneTask(taskId, gryzzlyTaskId)}
            />
          ) : (
            <div className="truncate text-[10px] text-gray-400">
              {projectName(lane.gryzzlyProjectId)}
            </div>
          );

        return (
        <div key={lane.laneKey} className="flex items-center gap-2">
          {/* `min-w-0` instead of `truncate` on the cell: the label line clips itself,
              and an overflow-hidden cell would cut the picker's dropdown off. */}
          <div className="w-48 min-w-0 shrink-0 text-xs" title={lane.label}>
            <div className="truncate font-medium text-gray-700">{lane.label}</div>
            {projectCell}
          </div>
          <div className="relative h-6 flex-1 rounded bg-gray-50">
            {/* Quarter boundaries, so the arbitration grid is visible behind the bars. */}
            {quarters.map((q) => (
              <div
                key={q.index}
                className="absolute top-0 bottom-0 border-l border-gray-200"
                style={{ left: `${pct(q.startMin)}%` }}
              />
            ))}
            {lane.intervals.map((iv) => (
              <div
                key={`${iv.startMin}-${iv.endMin}`}
                className="absolute top-1 bottom-1 rounded bg-blue-400/70"
                style={{
                  left: `${pct(iv.startMin)}%`,
                  width: `${Math.max(0.6, ((iv.endMin - iv.startMin) / span) * 100)}%`,
                }}
                title={`${lane.label} — ${hm(iv.startMin)}–${hm(iv.endMin)} (${iv.endMin - iv.startMin} min)`}
              />
            ))}
          </div>
          {lane.outsideMinutes > 0 && (
            <span
              className="w-16 shrink-0 text-right text-[10px] text-amber-600"
              title="Traces hors plage horaire — non déclarées"
            >
              +{lane.outsideMinutes} min
            </span>
          )}
        </div>
        );
      })}
    </div>
  );
}
