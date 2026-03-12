import React from 'react';
import type { Assignment, Conflict, Developer, Milestone, Project } from '@domain/index';

type TimelineProps = {
  readonly projects: readonly Project[];
  readonly assignments: readonly Assignment[];
  readonly developers: readonly Developer[];
  readonly milestones: readonly Milestone[];
  readonly conflicts: readonly Conflict[];
};

type DateRange = {
  readonly start: Date;
  readonly end: Date;
};

type ZoomLevel = 'month' | 'week' | 'day';

type TimeBucket = {
  readonly start: Date;
  readonly end: Date;
  readonly key: string;
  readonly label: string;
};

type HeaderSegment = {
  readonly key: string;
  readonly label: string;
  readonly span: number;
};

const toDate = (dateStr: string): Date => new Date(`${dateStr}T00:00:00`);

const getDateRange = (projects: readonly Project[]): DateRange | null => {
  if (projects.length === 0) {
    return null;
  }

  const dates = projects.flatMap((project) => [
    toDate(project.startDate),
    toDate(project.endDate),
  ]);

  const minDate = new Date(Math.min(...dates.map((d) => d.getTime())));
  const maxDate = new Date(Math.max(...dates.map((d) => d.getTime())));

  return { start: minDate, end: maxDate };
};

const startOfMonth = (date: Date): Date => new Date(date.getFullYear(), date.getMonth(), 1);

const endOfMonth = (date: Date): Date => new Date(date.getFullYear(), date.getMonth() + 1, 0);

const addDays = (date: Date, amount: number): Date => {
  const next = new Date(date);
  next.setDate(next.getDate() + amount);
  return next;
};

const addMonths = (date: Date, amount: number): Date => {
  const next = new Date(date);
  next.setMonth(next.getMonth() + amount);
  return next;
};

const getWeekStart = (date: Date): Date => {
  const day = date.getDay();
  const diff = day === 0 ? -6 : 1 - day;
  return addDays(new Date(date.getFullYear(), date.getMonth(), date.getDate()), diff);
};

const getWeekEnd = (date: Date): Date => addDays(getWeekStart(date), 6);

const formatDateKey = (date: Date): string => {
  const year = date.getFullYear();
  const month = `${date.getMonth() + 1}`.padStart(2, '0');
  const day = `${date.getDate()}`.padStart(2, '0');
  return `${year}-${month}-${day}`;
};

const formatMonthLabel = (date: Date): string =>
  date.toLocaleString('en-US', { month: 'short', year: '2-digit' });

const formatWeekLabel = (date: Date): string => {
  const day = `${date.getDate()}`.padStart(2, '0');
  const month = `${date.getMonth() + 1}`.padStart(2, '0');
  return `Wk ${day}/${month}`;
};

const formatDayLabel = (date: Date): string => `${date.getDate()}`.padStart(2, '0');

const isDateInRange = (date: Date, start: Date, end: Date): boolean =>
  date.getTime() >= start.getTime() && date.getTime() <= end.getTime();

const buildMonthBuckets = (range: DateRange): readonly TimeBucket[] => {
  const buckets: TimeBucket[] = [];
  let current = startOfMonth(range.start);
  const last = endOfMonth(range.end);

  while (current.getTime() <= last.getTime()) {
    const start = startOfMonth(current);
    const end = endOfMonth(current);
    buckets.push({
      start,
      end,
      key: formatDateKey(start),
      label: formatMonthLabel(start),
    });
    current = addMonths(current, 1);
  }

  return buckets;
};

const buildWeekBuckets = (range: DateRange): readonly TimeBucket[] => {
  const buckets: TimeBucket[] = [];
  let current = getWeekStart(range.start);
  const last = getWeekEnd(range.end);

  while (current.getTime() <= last.getTime()) {
    const start = getWeekStart(current);
    const end = getWeekEnd(current);
    buckets.push({
      start,
      end,
      key: formatDateKey(start),
      label: formatWeekLabel(start),
    });
    current = addDays(current, 7);
  }

  return buckets;
};

const buildDayBuckets = (range: DateRange): readonly TimeBucket[] => {
  const buckets: TimeBucket[] = [];
  let current = new Date(range.start.getFullYear(), range.start.getMonth(), range.start.getDate());
  const last = new Date(range.end.getFullYear(), range.end.getMonth(), range.end.getDate());

  while (current.getTime() <= last.getTime()) {
    buckets.push({
      start: current,
      end: current,
      key: formatDateKey(current),
      label: formatDayLabel(current),
    });
    current = addDays(current, 1);
  }

  return buckets;
};

const buildHeaderSegments = (
  buckets: readonly TimeBucket[],
  getKey: (bucket: TimeBucket) => string,
  getLabel: (bucket: TimeBucket) => string,
): readonly HeaderSegment[] =>
  buckets.reduce<HeaderSegment[]>((acc, bucket) => {
    const key = getKey(bucket);
    const label = getLabel(bucket);
    const last = acc[acc.length - 1];
    if (!last || last.key !== key) {
      return [...acc, { key, label, span: 1 }];
    }
    const updated = { ...last, span: last.span + 1 };
    return [...acc.slice(0, -1), updated];
  }, []);

const getAssignmentsForProjectAndRange = (
  projectId: string,
  rangeStart: Date,
  rangeEnd: Date,
  assignments: readonly Assignment[],
  developers: readonly Developer[],
): readonly string[] => {
  const matching = assignments.filter((assignment) => {
    if (assignment.projectId !== projectId) {
      return false;
    }
    const date = toDate(assignment.date);
    return isDateInRange(date, rangeStart, rangeEnd);
  });

  return matching.map((assignment) => {
    const dev = developers.find((developer) => developer.id === assignment.developerId);
    const initial = dev?.displayName?.charAt(0).toUpperCase() ?? '?';
    const halfDayIndicator = assignment.halfDay === 'morning' ? 'AM' : 'PM';
    return `${initial}${halfDayIndicator}`;
  });
};

const getMilestonesForProjectAndRange = (
  projectId: string,
  rangeStart: Date,
  rangeEnd: Date,
  milestones: readonly Milestone[],
): readonly Milestone[] =>
  milestones.filter((milestone) => {
    if (milestone.projectId !== projectId) {
      return false;
    }
    const date = toDate(milestone.date);
    return isDateInRange(date, rangeStart, rangeEnd);
  });

const formatMilestoneSummary = (milestones: readonly Milestone[]): string => {
  if (milestones.length === 0) {
    return '';
  }
  if (milestones.length === 1) {
    return '◆';
  }
  return `◆${milestones.length}`;
};

const formatMilestoneTitle = (milestones: readonly Milestone[]): string =>
  milestones
    .map((milestone) => `${milestone.name} (${milestone.date})`)
    .join(', ');

const hasConflictForProjectAndRange = (
  projectId: string,
  rangeStart: Date,
  rangeEnd: Date,
  conflicts: readonly Conflict[],
): boolean =>
  conflicts.some((conflict) => {
    if (!conflict.projectIds || !conflict.projectIds.includes(projectId)) {
      return false;
    }
    return conflict.dates.some((date) => isDateInRange(toDate(date), rangeStart, rangeEnd));
  });

const getAssignmentsSummary = (initials: readonly string[]): string => {
  if (initials.length === 0) {
    return '';
  }
  if (initials.length <= 2) {
    return initials.join(' ');
  }
  return `${initials.slice(0, 2).join(' ')} +${initials.length - 2}`;
};

const getProjectRangeIndexes = (
  buckets: readonly TimeBucket[],
  startDate: Date,
  endDate: Date,
): { readonly startIndex: number; readonly endIndex: number } | null => {
  const startIndex = buckets.findIndex((bucket) => isDateInRange(startDate, bucket.start, bucket.end));
  const endIndex = [...buckets]
    .reverse()
    .findIndex((bucket) => isDateInRange(endDate, bucket.start, bucket.end));
  if (startIndex === -1 || endIndex === -1) {
    return null;
  }
  const normalizedEndIndex = buckets.length - 1 - endIndex;
  return { startIndex, endIndex: normalizedEndIndex };
};

const getColumnWidth = (zoom: ZoomLevel): number => {
  switch (zoom) {
    case 'day':
      return 36;
    case 'week':
      return 64;
    case 'month':
    default:
      return 90;
  }
};

export const Timeline: React.FC<TimelineProps> = ({
  projects,
  assignments,
  developers,
  milestones,
  conflicts,
}) => {
  const [zoom, setZoom] = React.useState<ZoomLevel>('month');
  const gridRef = React.useRef<HTMLDivElement | null>(null);
  const dateRange = getDateRange(projects);

  if (!dateRange) {
    return (
      <section className="timeline">
        <h2>Timeline</h2>
        <p>No projects to display.</p>
      </section>
    );
  }

  const rangeByZoom: DateRange =
    zoom === 'month'
      ? { start: startOfMonth(dateRange.start), end: endOfMonth(dateRange.end) }
      : zoom === 'week'
      ? { start: getWeekStart(dateRange.start), end: getWeekEnd(dateRange.end) }
      : dateRange;

  const buckets =
    zoom === 'month'
      ? buildMonthBuckets(rangeByZoom)
      : zoom === 'week'
      ? buildWeekBuckets(rangeByZoom)
      : buildDayBuckets(rangeByZoom);

  const columnWidth = getColumnWidth(zoom);
  const gridTemplateColumns = `220px repeat(${buckets.length}, ${columnWidth}px)`;
  const headerRows = zoom === 'day' ? 3 : zoom === 'week' ? 2 : 1;
  const today = new Date();
  const todayDate = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const todayIndex = buckets.findIndex((bucket) =>
    isDateInRange(todayDate, bucket.start, bucket.end),
  );

  const monthSegments =
    zoom === 'month'
      ? buckets.map((bucket) => ({ key: bucket.key, label: bucket.label, span: 1 }))
      : buildHeaderSegments(
          buckets,
          (bucket) => `${bucket.start.getFullYear()}-${bucket.start.getMonth()}`,
          (bucket) => formatMonthLabel(bucket.start),
        );

  const weekSegments =
    zoom === 'week'
      ? buckets.map((bucket) => ({ key: bucket.key, label: bucket.label, span: 1 }))
      : zoom === 'day'
      ? buildHeaderSegments(
          buckets,
          (bucket) => formatDateKey(getWeekStart(bucket.start)),
          (bucket) => formatWeekLabel(getWeekStart(bucket.start)),
        )
      : [];
  const monthSegmentPositions = monthSegments.reduce<
    { readonly items: readonly (HeaderSegment & { readonly start: number })[]; readonly next: number }
  >(
    (acc, segment) => ({
      items: [...acc.items, { ...segment, start: acc.next }],
      next: acc.next + segment.span,
    }),
    { items: [], next: 2 },
  ).items;
  const weekSegmentPositions = weekSegments.reduce<
    { readonly items: readonly (HeaderSegment & { readonly start: number })[]; readonly next: number }
  >(
    (acc, segment) => ({
      items: [...acc.items, { ...segment, start: acc.next }],
      next: acc.next + segment.span,
    }),
    { items: [], next: 2 },
  ).items;
  React.useEffect(() => {
    if (!gridRef.current || projects.length === 0) {
      return;
    }
    if (zoom === 'month') {
      return;
    }
    const firstProject = projects[0];
    const projectStart = toDate(firstProject.startDate);
    const projectEnd = toDate(firstProject.endDate);
    const rangeIndexes = getProjectRangeIndexes(buckets, projectStart, projectEnd);
    if (!rangeIndexes) {
      return;
    }
    const targetLeft = Math.max(0, rangeIndexes.startIndex * columnWidth);
    gridRef.current.scrollTo({ left: targetLeft });
  }, [zoom, buckets, columnWidth, projects]);

  return (
    <section className="timeline">
      <div className="timeline-controls">
        <h2>Timeline</h2>
        <div className="timeline-zoom">
          <button
            type="button"
            className={`tab-button ${zoom === 'month' ? 'active' : ''}`}
            onClick={() => setZoom('month')}
          >
            Month
          </button>
          <button
            type="button"
            className={`tab-button ${zoom === 'week' ? 'active' : ''}`}
            onClick={() => setZoom('week')}
          >
            Week
          </button>
          <button
            type="button"
            className={`tab-button ${zoom === 'day' ? 'active' : ''}`}
            onClick={() => setZoom('day')}
          >
            Day
          </button>
        </div>
      </div>
      <div className="timeline-grid gantt-grid" style={{ gridTemplateColumns }} ref={gridRef}>
        <div className="timeline-header">
          <div className="timeline-cell header project-name" style={{ gridColumn: 1, gridRow: 1 }}>
            Project
          </div>
          {monthSegmentPositions.map((segment) => (
            <div
              key={`month-${segment.key}`}
              className="timeline-cell header gantt-header"
              style={{ gridColumn: `${segment.start} / span ${segment.span}`, gridRow: 1 }}
            >
              {segment.label}
            </div>
          ))}
        </div>

        {weekSegments.length > 0 ? (
          <div className="timeline-header">
            <div className="timeline-cell header project-name" style={{ gridColumn: 1, gridRow: 2 }}>
              {' '}
            </div>
            {weekSegmentPositions.map((segment) => (
              <div
                key={`week-${segment.key}`}
                className="timeline-cell header gantt-header secondary"
                style={{ gridColumn: `${segment.start} / span ${segment.span}`, gridRow: 2 }}
              >
                {segment.label}
              </div>
            ))}
          </div>
        ) : null}

        {zoom === 'day' ? (
          <div className="timeline-header">
            <div className="timeline-cell header project-name" style={{ gridColumn: 1, gridRow: 3 }}>
              {' '}
            </div>
            {buckets.map((bucket, index) => (
              <div
                key={`day-${bucket.key}`}
                className="timeline-cell header gantt-header tertiary"
                style={{ gridColumn: index + 2, gridRow: 3 }}
              >
                {bucket.label}
              </div>
            ))}
          </div>
        ) : null}

        {todayIndex >= 0 ? (
          <div
            className="gantt-today"
            style={{
              gridColumn: `${todayIndex + 2} / span 1`,
              gridRow: `1 / span ${headerRows + projects.length}`,
            }}
          />
        ) : null}

        {projects.map((project, index) => {
          const projectStart = toDate(project.startDate);
          const projectEnd = toDate(project.endDate);
          const rangeIndexes = getProjectRangeIndexes(buckets, projectStart, projectEnd);
          const gridRow = headerRows + index + 1;
          return (
            <div key={project.id} className="timeline-row">
              <div
                className="timeline-cell project-name"
                style={{ gridColumn: 1, gridRow }}
              >
                {project.name}
              </div>
              {buckets.map((bucket, bucketIndex) => {
                const isInRange = isDateInRange(bucket.start, projectStart, projectEnd);
                const isStart = rangeIndexes?.startIndex === bucketIndex;
                const isEnd = rangeIndexes?.endIndex === bucketIndex;
                const initials = getAssignmentsForProjectAndRange(
                  project.id,
                  bucket.start,
                  bucket.end,
                  assignments,
                  developers,
                );
                const milestoneMatches = getMilestonesForProjectAndRange(
                  project.id,
                  bucket.start,
                  bucket.end,
                  milestones,
                );
                const hasConflict = hasConflictForProjectAndRange(
                  project.id,
                  bucket.start,
                  bucket.end,
                  conflicts,
                );
                const summary = getAssignmentsSummary(initials);
                const milestoneSummary = formatMilestoneSummary(milestoneMatches);
                const milestoneTitle = formatMilestoneTitle(milestoneMatches);
                return (
                  <div
                    key={`${project.id}-${bucket.key}`}
                    className={[
                      'timeline-cell',
                      'gantt-cell',
                      isInRange ? 'gantt-bar' : 'empty',
                      isStart ? 'gantt-start' : '',
                      isEnd ? 'gantt-end' : '',
                    ]
                      .filter(Boolean)
                      .join(' ')}
                    style={{ gridColumn: bucketIndex + 2, gridRow }}
                  >
                    {hasConflict ? (
                      <span className="gantt-conflict" title="Conflict detected">
                        !
                      </span>
                    ) : null}
                    {milestoneSummary ? (
                      <span className="gantt-milestone" title={milestoneTitle}>
                        {milestoneSummary}
                      </span>
                    ) : null}
                    {summary ? <span className="gantt-assignment">{summary}</span> : null}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </section>
  );
};
