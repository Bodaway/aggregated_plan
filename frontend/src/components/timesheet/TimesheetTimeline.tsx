import type { AttributedBlock } from '@/hooks/use-timesheet';
import { projectColor } from './project-colors';

const AM_START = 8 * 60;
const AM_END = 12 * 60;
const PM_START = 13 * 60;
const PM_END = 17 * 60;

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
  { label: 'Morning', start: AM_START, end: AM_END },
  { label: 'Afternoon', start: PM_START, end: PM_END },
];

function blockClasses(kind: AttributedBlock['kind'], projectId: string | null): string {
  if (kind === 'MEETING') return 'bg-slate-400 bg-[repeating-linear-gradient(45deg,transparent,transparent_4px,rgba(0,0,0,0.12)_4px,rgba(0,0,0,0.12)_8px)]';
  if (kind === 'OUT_OF_OFFICE') return 'bg-gray-200';
  return projectColor(projectId);
}

function HalfDay({ win, blocks }: { win: WindowDef; blocks: AttributedBlock[] }) {
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
          const label = b.kind === 'MEETING' ? 'meet' : (b.gryzzlyProjectId ?? '??');
          return (
            <div
              key={`${b.startTime}-${i}`}
              data-block
              title={`${label} · ${b.hours.toFixed(2)}h`}
              className={`absolute top-1 bottom-1 rounded text-[10px] text-white/90 px-1 overflow-hidden whitespace-nowrap ${blockClasses(b.kind, b.gryzzlyProjectId)}`}
              style={{ left: `${leftPct}%`, width: `${widthPct}%` }}
            >
              {widthPct > 10 ? label : ''}
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function TimesheetTimeline({ blocks }: { blocks: AttributedBlock[] }) {
  if (blocks.length === 0) {
    return <div className="text-sm text-gray-400 italic py-4">No activity reconstructed for this day.</div>;
  }
  return (
    <div className="flex gap-6">
      {WINDOWS.map((win) => (
        <HalfDay key={win.label} win={win} blocks={blocks} />
      ))}
    </div>
  );
}
