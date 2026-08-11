import type { AttributedBlock, ProjectOption, UnresolvedSignal } from '@/hooks/use-timesheet';
import { projectColor } from './project-colors';

const AM_START = 8 * 60;
const AM_END = 12 * 60;
const PM_START = 13 * 60;
const PM_END = 17 * 60;

/** What a WORK block reads when no Gryzzly project could be resolved for it. */
export const UNATTRIBUTED_LABEL = 'Non attribué';

/** Below this share of a half-day, a bar is too narrow for its project name: a clipped
 *  half-character says less than nothing, so it renders bare. */
const MIN_WIDTH_PCT_FOR_LABEL = 10;
/** The second line (task name) needs more room than the first: it is smaller, so more
 *  characters fit per pixel and a half-word is likelier. */
const MIN_WIDTH_PCT_FOR_ORIGIN = 18;

function timeToMinutes(iso: string): number {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return 0;
  return d.getHours() * 60 + d.getMinutes();
}

interface WindowDef {
  label: string;
  start: number;
  end: number;
}
const WINDOWS: WindowDef[] = [
  { label: 'Matin', start: AM_START, end: AM_END },
  { label: 'Après-midi', start: PM_START, end: PM_END },
];

function blockClasses(kind: AttributedBlock['kind'], projectId: string | null): string {
  if (kind === 'MEETING') return 'bg-slate-400 bg-[repeating-linear-gradient(45deg,transparent,transparent_4px,rgba(0,0,0,0.12)_4px,rgba(0,0,0,0.12)_8px)]';
  if (kind === 'OUT_OF_OFFICE') return 'bg-gray-200';
  return projectColor(projectId);
}

/** A raw project id explains nothing; prefer its catalog name, fall back to the id. */
function blockLabel(b: AttributedBlock, projectNames: Map<string, string>): string {
  if (b.kind === 'MEETING') return 'réu';
  if (b.kind === 'OUT_OF_OFFICE') return 'absence';
  if (!b.gryzzlyProjectId) return UNATTRIBUTED_LABEL;
  return projectNames.get(b.gryzzlyProjectId) ?? b.gryzzlyProjectId;
}

/** The block's secondary label — the name of what it came from (task title, meeting
 *  subject). Empty or whitespace-only reads as absent: one line beats a blank one. */
function blockOriginLabel(b: AttributedBlock): string | null {
  const trimmed = b.originLabel?.trim();
  return trimmed ? trimmed : null;
}

/** Blocks and unresolved signals share `sourceRef` (`wl:<uuid>`), so an unattributed bar
 *  can name the worklog notes behind it instead of staying anonymous. */
function blockTitle(
  b: AttributedBlock,
  label: string,
  originLabel: string | null,
  unresolvedLabels: Map<string, string>,
): string {
  const notes = b.sourceRefs
    .map((ref) => unresolvedLabels.get(ref))
    .filter((note): note is string => Boolean(note));
  // Both names always reach the tooltip, even when the bar is too narrow to show either.
  const head = [label, originLabel, `${b.hours.toFixed(2)}h`]
    .filter((part): part is string => Boolean(part))
    .join(' · ');
  return notes.length > 0 ? `${head} — ${notes.join(' · ')}` : head;
}

interface HalfDayProps {
  win: WindowDef;
  blocks: AttributedBlock[];
  projectNames: Map<string, string>;
  unresolvedLabels: Map<string, string>;
}

function HalfDay({ win, blocks, projectNames, unresolvedLabels }: HalfDayProps) {
  const duration = win.end - win.start;
  const inWindow = blocks.filter((b) => timeToMinutes(b.endTime) > win.start && timeToMinutes(b.startTime) < win.end);
  return (
    <div className="flex-1">
      <div className="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-1">{win.label}</div>
      <div className="relative h-16 rounded bg-gray-50 border border-gray-200 overflow-hidden">
        {inWindow.map((b, i) => {
          const s = Math.max(timeToMinutes(b.startTime), win.start);
          const e = Math.min(timeToMinutes(b.endTime), win.end);
          const leftPct = ((s - win.start) / duration) * 100;
          const widthPct = Math.max(((e - s) / duration) * 100, 1);
          const label = blockLabel(b, projectNames);
          const originLabel = blockOriginLabel(b);
          // `h-16` minus `top-1 bottom-1` leaves ~56px: two `leading-tight` lines fit
          // without changing the bar's geometry.
          const showLabel = widthPct > MIN_WIDTH_PCT_FOR_LABEL;
          const showOrigin = originLabel !== null && widthPct > MIN_WIDTH_PCT_FOR_ORIGIN;
          return (
            <div
              key={`${b.startTime}-${i}`}
              data-block
              title={blockTitle(b, label, originLabel, unresolvedLabels)}
              className={`absolute top-1 bottom-1 rounded text-white/90 px-1 overflow-hidden ${blockClasses(b.kind, b.gryzzlyProjectId)}`}
              style={{ left: `${leftPct}%`, width: `${widthPct}%` }}
            >
              {showLabel && (
                <div className="text-[10px] leading-tight overflow-hidden whitespace-nowrap text-ellipsis">
                  {label}
                </div>
              )}
              {showOrigin && (
                <div className="text-[9px] leading-tight text-white/70 overflow-hidden whitespace-nowrap text-ellipsis">
                  {originLabel}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

interface TimesheetTimelineProps {
  blocks: AttributedBlock[];
  /** Catalog names for resolved blocks. Omit and bars fall back to the raw project id. */
  projects?: ProjectOption[];
  /** Explains the unattributed bars in their tooltip. Omit and only hours are shown. */
  unresolved?: UnresolvedSignal[];
}

export function TimesheetTimeline({ blocks, projects = [], unresolved = [] }: TimesheetTimelineProps) {
  if (blocks.length === 0) {
    return <div className="text-sm text-gray-400 italic py-4">Aucune activité reconstruite pour ce jour.</div>;
  }
  const projectNames = new Map(projects.map((p) => [p.id, p.label]));
  const unresolvedLabels = new Map(unresolved.map((u) => [u.sourceRef, u.label]));
  return (
    <div className="flex gap-6">
      {WINDOWS.map((win) => (
        <HalfDay
          key={win.label}
          win={win}
          blocks={blocks}
          projectNames={projectNames}
          unresolvedLabels={unresolvedLabels}
        />
      ))}
    </div>
  );
}
