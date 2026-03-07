import type { ActivitySlot } from '@/hooks/use-activity';

interface ActivityTimelineProps {
  readonly slots: readonly ActivitySlot[];
}

/** Morning time boundaries (8:00 - 12:00) in minutes from midnight. */
const AM_START = 8 * 60; // 480
const AM_END = 12 * 60; // 720

/** Afternoon time boundaries (13:00 - 17:00) in minutes from midnight. */
const PM_START = 13 * 60; // 780
const PM_END = 17 * 60; // 1020

/** Color palette for different activity slots. */
const SLOT_COLORS = [
  'bg-blue-400',
  'bg-green-400',
  'bg-purple-400',
  'bg-amber-400',
  'bg-rose-400',
  'bg-cyan-400',
  'bg-orange-400',
  'bg-teal-400',
] as const;

/** Parses a time string and returns minutes from midnight. */
function timeToMinutes(timeStr: string): number {
  try {
    const d = new Date(timeStr);
    if (!isNaN(d.getTime())) {
      return d.getHours() * 60 + d.getMinutes();
    }
  } catch {
    // fall through
  }
  return 0;
}

/** Formats minutes from midnight to HH:mm. */
function minutesToTime(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
}

/** Renders one half-day block (AM or PM). */
function TimeBlock({
  label,
  blockStart,
  blockEnd,
  slots,
}: {
  readonly label: string;
  readonly blockStart: number;
  readonly blockEnd: number;
  readonly slots: readonly ActivitySlot[];
}) {
  const blockDuration = blockEnd - blockStart;
  const hours = Array.from(
    { length: Math.floor(blockDuration / 60) + 1 },
    (_, i) => blockStart + i * 60
  );

  // Filter slots to ones that overlap this block
  const relevantSlots = slots.filter(slot => {
    const start = timeToMinutes(slot.startTime);
    const end = slot.endTime ? timeToMinutes(slot.endTime) : start + 30; // default 30min
    return start < blockEnd && end > blockStart;
  });

  return (
    <div className="flex-1">
      <h4 className="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-2">
        {label} ({minutesToTime(blockStart)} - {minutesToTime(blockEnd)})
      </h4>
      <div className="relative bg-gray-50 rounded-lg border border-gray-200 h-16">
        {/* Hour markers */}
        {hours.map(h => {
          const leftPct = ((h - blockStart) / blockDuration) * 100;
          return (
            <div
              key={h}
              className="absolute top-0 bottom-0 border-l border-gray-200"
              style={{ left: `${leftPct}%` }}
            >
              <span className="absolute -top-4 -translate-x-1/2 text-xs text-gray-400">
                {minutesToTime(h)}
              </span>
            </div>
          );
        })}

        {/* Activity bars */}
        {relevantSlots.map((slot, idx) => {
          const rawStart = timeToMinutes(slot.startTime);
          const rawEnd = slot.endTime ? timeToMinutes(slot.endTime) : rawStart + 30;
          const clampedStart = Math.max(rawStart, blockStart);
          const clampedEnd = Math.min(rawEnd, blockEnd);
          const leftPct = ((clampedStart - blockStart) / blockDuration) * 100;
          const widthPct = ((clampedEnd - clampedStart) / blockDuration) * 100;
          const colorClass = SLOT_COLORS[idx % SLOT_COLORS.length];

          return (
            <div
              key={slot.id}
              className={`absolute top-1 bottom-1 ${colorClass} rounded opacity-80 flex items-center px-1 overflow-hidden`}
              style={{ left: `${leftPct}%`, width: `${Math.max(widthPct, 1)}%` }}
              title={`${slot.task?.title ?? 'Activity'} (${minutesToTime(clampedStart)} - ${minutesToTime(clampedEnd)})`}
            >
              {widthPct > 10 && (
                <span className="text-xs text-white font-medium truncate">
                  {slot.task?.title ?? 'Activity'}
                </span>
              )}
            </div>
          );
        })}

        {/* Empty state */}
        {relevantSlots.length === 0 && (
          <div className="flex items-center justify-center h-full">
            <span className="text-xs text-gray-400">No activity</span>
          </div>
        )}
      </div>
    </div>
  );
}

export function ActivityTimeline({ slots }: ActivityTimelineProps) {
  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4">
      <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider mb-6">
        Day Timeline
      </h3>
      <div className="flex gap-6">
        <TimeBlock label="Morning" blockStart={AM_START} blockEnd={AM_END} slots={slots} />
        <TimeBlock label="Afternoon" blockStart={PM_START} blockEnd={PM_END} slots={slots} />
      </div>
    </div>
  );
}
