import type {
  Assignment,
  Availability,
  Conflict,
  EntityId,
  IsoDateString,
} from '@aggregated-plan/shared-types';
import {
  addDays,
  compareIsoDates,
  getWeekStart,
  listIsoDatesInRange,
} from '@aggregated-plan/shared-utils';

export type CapacitySnapshot = {
  readonly developerId: EntityId;
  readonly capacityHalfDaysPerWeek: number;
};

export type ConflictInput = {
  readonly assignments: readonly Assignment[];
  readonly availabilities: readonly Availability[];
  readonly capacities: readonly CapacitySnapshot[];
};

const buildOverlapConflicts = (assignments: readonly Assignment[]): Conflict[] => {
  const grouped = assignments.reduce<Map<string, Assignment[]>>((acc, assignment) => {
    const key = `${assignment.developerId}|${assignment.date}|${assignment.halfDay}`;
    const existing = acc.get(key) ?? [];
    acc.set(key, [...existing, assignment]);
    return acc;
  }, new Map());

  return Array.from(grouped.values())
    .filter((group) => group.length > 1)
    .map((group) => ({
      type: 'overlap' as const,
      developerId: group[0].developerId,
      message: 'Developer has overlapping assignments.',
      dates: [group[0].date],
      halfDay: group[0].halfDay,
      projectIds: group.map((assignment) => assignment.projectId),
    }));
};

const isDateInAvailability = (date: IsoDateString, availability: Availability): boolean =>
  compareIsoDates(date, availability.startDate) >= 0 &&
  compareIsoDates(date, availability.endDate) <= 0;

const buildAvailabilityConflicts = (
  assignments: readonly Assignment[],
  availabilities: readonly Availability[],
): Conflict[] => {
  return assignments
    .filter((assignment) =>
      availabilities.some(
        (availability) =>
          availability.developerId === assignment.developerId &&
          isDateInAvailability(assignment.date, availability),
      ),
    )
    .map((assignment) => ({
      type: 'unavailability' as const,
      developerId: assignment.developerId,
      message: 'Assignment overlaps an unavailable period.',
      dates: [assignment.date],
      halfDay: assignment.halfDay,
      projectIds: [assignment.projectId],
    }));
};

const buildCapacityConflicts = (
  assignments: readonly Assignment[],
  availabilities: readonly Availability[],
  capacities: readonly CapacitySnapshot[],
): Conflict[] => {
  const assignmentsByDeveloper = assignments.reduce<Map<EntityId, Assignment[]>>(
    (acc, assignment) => {
      const existing = acc.get(assignment.developerId) ?? [];
      acc.set(assignment.developerId, [...existing, assignment]);
      return acc;
    },
    new Map(),
  );

  const capacityByDeveloper = capacities.reduce<Map<EntityId, number>>(
    (acc, capacity) => {
      acc.set(capacity.developerId, capacity.capacityHalfDaysPerWeek);
      return acc;
    },
    new Map(),
  );

  return Array.from(assignmentsByDeveloper.entries()).flatMap(([developerId, devAssignments]) => {
    const assignmentsByWeek = devAssignments.reduce<Map<IsoDateString, Assignment[]>>(
      (acc, assignment) => {
        const weekStart = getWeekStart(assignment.date);
        const existing = acc.get(weekStart) ?? [];
        acc.set(weekStart, [...existing, assignment]);
        return acc;
      },
      new Map(),
    );

    return Array.from(assignmentsByWeek.entries()).flatMap(([weekStart, weekAssignments]) => {
      const weekEnd = addDays(weekStart, 6);
      const weekDates = listIsoDatesInRange(weekStart, weekEnd);
      const unavailableDates = availabilities
        .filter((availability) => availability.developerId === developerId)
        .flatMap((availability) =>
          listIsoDatesInRange(availability.startDate, availability.endDate),
        )
        .filter(
          (date) =>
            compareIsoDates(date, weekStart) >= 0 && compareIsoDates(date, weekEnd) <= 0,
        );
      const uniqueUnavailableDates = Array.from(new Set(unavailableDates));
      const unavailableHalfDays = uniqueUnavailableDates.length * 2;
      const capacity = capacityByDeveloper.get(developerId) ?? 10;
      const availableCapacity = Math.max(0, capacity - unavailableHalfDays);
      const assignedHalfDays = weekAssignments.length;

      if (assignedHalfDays <= availableCapacity) {
        return [];
      }

      return [
        {
          type: 'capacity' as const,
          developerId,
          message: 'Developer exceeds weekly capacity.',
          dates: weekDates,
          weekStart,
          assignedHalfDays,
          capacityHalfDays: availableCapacity,
        },
      ];
    });
  });
};

export const detectConflicts = (input: ConflictInput): Conflict[] => [
  ...buildOverlapConflicts(input.assignments),
  ...buildAvailabilityConflicts(input.assignments, input.availabilities),
  ...buildCapacityConflicts(input.assignments, input.availabilities, input.capacities),
];
