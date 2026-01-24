import React from 'react';
import type { Assignment, Developer, Project } from '@domain/index';

type TimelineProps = {
  readonly projects: readonly Project[];
  readonly assignments: readonly Assignment[];
  readonly developers: readonly Developer[];
};

type DateRange = {
  readonly start: Date;
  readonly end: Date;
};

const parseDateString = (dateStr: string): Date => new Date(dateStr);

const getDateRange = (projects: readonly Project[]): DateRange | null => {
  if (projects.length === 0) {
    return null;
  }

  const dates = projects.flatMap((p) => [
    parseDateString(p.startDate),
    parseDateString(p.endDate),
  ]);

  const minDate = new Date(Math.min(...dates.map((d) => d.getTime())));
  const maxDate = new Date(Math.max(...dates.map((d) => d.getTime())));

  return { start: minDate, end: maxDate };
};

const generateDateColumns = (range: DateRange): readonly Date[] => {
  const dates: Date[] = [];
  const current = new Date(range.start);

  while (current <= range.end) {
    if (current.getDay() !== 0 && current.getDay() !== 6) {
      dates.push(new Date(current));
    }
    current.setDate(current.getDate() + 1);
  }

  return dates;
};

const formatDateHeader = (date: Date): string => {
  const day = date.getDate().toString().padStart(2, '0');
  const month = (date.getMonth() + 1).toString().padStart(2, '0');
  return `${day}/${month}`;
};

const formatDateKey = (date: Date): string => {
  const year = date.getFullYear();
  const month = (date.getMonth() + 1).toString().padStart(2, '0');
  const day = date.getDate().toString().padStart(2, '0');
  return `${year}-${month}-${day}`;
};

const getAssignmentsForProjectAndDate = (
  projectId: string,
  dateKey: string,
  assignments: readonly Assignment[],
  developers: readonly Developer[],
): readonly string[] => {
  const matching = assignments.filter(
    (a) => a.projectId === projectId && a.date === dateKey,
  );

  return matching.map((a) => {
    const dev = developers.find((d) => d.id === a.developerId);
    const initial = dev?.displayName?.charAt(0).toUpperCase() ?? '?';
    const halfDayIndicator = a.halfDay === 'morning' ? 'AM' : 'PM';
    return `${initial}${halfDayIndicator}`;
  });
};

export const Timeline: React.FC<TimelineProps> = ({
  projects,
  assignments,
  developers,
}) => {
  const dateRange = getDateRange(projects);

  if (!dateRange) {
    return (
      <section className="timeline">
        <h2>Timeline</h2>
        <p>No projects to display.</p>
      </section>
    );
  }

  const dateColumns = generateDateColumns(dateRange);
  const gridTemplateColumns = `200px repeat(${dateColumns.length}, 60px)`;

  return (
    <section className="timeline">
      <h2>Timeline</h2>
      <div
        className="timeline-grid"
        style={{ gridTemplateColumns }}
      >
        <div className="timeline-header">
          <div className="timeline-cell header project-name">Project</div>
          {dateColumns.map((date) => (
            <div key={formatDateKey(date)} className="timeline-cell header">
              {formatDateHeader(date)}
            </div>
          ))}
        </div>

        {projects.map((project) => (
          <div key={project.id} className="timeline-row">
            <div className="timeline-cell project-name">{project.name}</div>
            {dateColumns.map((date) => {
              const dateKey = formatDateKey(date);
              const projectStart = parseDateString(project.startDate);
              const projectEnd = parseDateString(project.endDate);
              const isInRange = date >= projectStart && date <= projectEnd;

              const devInitials = getAssignmentsForProjectAndDate(
                project.id,
                dateKey,
                assignments,
                developers,
              );

              if (!isInRange) {
                return (
                  <div key={dateKey} className="timeline-cell empty">
                    -
                  </div>
                );
              }

              if (devInitials.length > 0) {
                return (
                  <div key={dateKey} className="timeline-cell assigned">
                    {devInitials.join(' ')}
                  </div>
                );
              }

              return (
                <div key={dateKey} className="timeline-cell">
                  ·
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </section>
  );
};
