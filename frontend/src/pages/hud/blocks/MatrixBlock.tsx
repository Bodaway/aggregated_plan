import { useMemo } from 'react';
import { usePriorityMatrix } from '@/hooks/use-priority-matrix';
import { matrixBacklog, topOfMatrix } from '@/lib/matrix-top';
import { formatDeadlineLabel } from '@/lib/deadline-label';
import { formatDate } from '@/lib/date-utils';

interface MatrixBlockProps {
  /** Whether this block carries the HUD's one glow, as arbitrated by
   *  `useDominantBlock`. */
  readonly lit: boolean;
}

/** Rows drawn before this panel's fixed grid area runs out. Five, the same
 *  as `PressureBlock`'s `MAX_VISIBLE_DEADLINES`, because it is the same
 *  slot: this block took the area Pressure vacated when Pressure moved into
 *  the dominant cell. The label still states the true count. */
const MAX_VISIBLE_TASKS = 5;

/** What to do now: the critical band, then the urgent-and-important
 *  quadrant. Reads the same `priorityMatrix` query as the priority page, and
 *  the selection itself lives in `lib/matrix-top.ts` so both surfaces answer
 *  "the top of the matrix" the same way. */
export function MatrixBlock({ lit }: MatrixBlockProps) {
  const today = formatDate(new Date());
  const { data } = usePriorityMatrix();
  const top = useMemo(() => topOfMatrix(data), [data]);
  const backlog = useMemo(() => matrixBacklog(data), [data]);

  const panelClass = lit ? 'hud-panel hud-panel--lit hud-matrix' : 'hud-panel hud-matrix';

  return (
    <div className={panelClass} data-testid="matrix-block">
      <div className="hud-label">{top.length > 0 ? `▌ Priority · ${top.length} to do` : '▌ Priority'}</div>

      {top.length === 0 ? (
        <div className="hud-matrix__empty">Nothing at the top of the matrix</div>
      ) : (
        <div className="hud-matrix__list">
          {top.slice(0, MAX_VISIBLE_TASKS).map((t) => (
            <div key={t.id} className="hud-matrix__row" data-testid="matrix-task">
              <i
                className={t.critical ? 'hud-matrix__dot hud-matrix__dot--critical' : 'hud-matrix__dot'}
                data-testid="matrix-dot"
              />
              <span className="hud-matrix__what">{t.title}</span>
              {/* Only when there is one, and no pink even for today: this
                  panel ranks by importance, Pressure ranks by date, and
                  spending Pressure's one reserved colour here would make
                  two panels shout the same thing side by side. */}
              {t.deadline && <b>{formatDeadlineLabel(t.deadline, today)}</b>}
            </div>
          ))}
        </div>
      )}

      {/* Pinned to the foot of the panel the way Pressure pins its capacity
          gauge. It is not filler: this block shows one quadrant plus the
          criticals, and without a line saying what that leaves out, a short
          list reads as "nothing to do" rather than "nothing at the top". */}
      <div className="hud-matrix__backlog" data-testid="matrix-backlog">
        <div className="hud-matrix__backlog-caption">Below the line</div>
        <div className="hud-matrix__backlog-row">
          <span>
            Important <b>{backlog.important}</b>
          </span>
          <span>
            Urgent <b>{backlog.urgent}</b>
          </span>
          <span>
            Neither <b>{backlog.neither}</b>
          </span>
        </div>
      </div>
    </div>
  );
}
