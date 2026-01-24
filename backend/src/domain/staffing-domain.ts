import type {
  Assignment,
  EntityId,
  HalfDay,
  IsoDateString,
  Weekday,
  WeeklyAllocation,
} from '@aggregated-plan/shared-types';
import {
  addDays,
  compareIsoDates,
  listWeekStartsInRange,
} from '@aggregated-plan/shared-utils';
import { createDomainError } from './domain-errors';
import type { DomainError } from './domain-errors';
import { err, ok } from './result';
import type { Result } from './result';

export type CreateAssignmentParams = {
  readonly projectId: EntityId;
  readonly developerId: EntityId;
  readonly date: IsoDateString;
  readonly halfDay: HalfDay;
};

export type AssignmentContext = {
  readonly id: EntityId;
  readonly createdAt: IsoDateString;
};

export type AssignmentSeed = {
  readonly projectId: EntityId;
  readonly developerId: EntityId;
  readonly date: IsoDateString;
  readonly halfDay: HalfDay;
};

export type CreateWeeklyAllocationParams = {
  readonly projectId: EntityId;
  readonly developerId: EntityId;
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
  readonly halfDaysPerWeek: number;
  readonly preferredWeekdays?: readonly Weekday[];
};

export type WeeklyAllocationContext = {
  readonly id: EntityId;
  readonly createdAt: IsoDateString;
};

const DEFAULT_WEEKDAYS: readonly Weekday[] = [
  'monday',
  'tuesday',
  'wednesday',
  'thursday',
  'friday',
];

const WEEKDAY_OFFSETS: Record<Weekday, number> = {
  monday: 0,
  tuesday: 1,
  wednesday: 2,
  thursday: 3,
  friday: 4,
  saturday: 5,
  sunday: 6,
};

const ensureValidDateRange = (
  startDate: IsoDateString,
  endDate: IsoDateString,
): Result<null, DomainError> => {
  if (compareIsoDates(startDate, endDate) > 0) {
    return err(
      createDomainError('invalid-date-range', 'Start date must be before end date.'),
    );
  }
  return ok(null);
};

const normalizeWeekdays = (weekdays?: readonly Weekday[]): readonly Weekday[] =>
  weekdays && weekdays.length > 0 ? [...weekdays] : DEFAULT_WEEKDAYS;

export const createAssignment = (
  params: CreateAssignmentParams,
  context: AssignmentContext,
): Result<Assignment, DomainError> => {
  const assignment: Assignment = {
    id: context.id,
    projectId: params.projectId,
    developerId: params.developerId,
    date: params.date,
    halfDay: params.halfDay,
    createdAt: context.createdAt,
  };

  return ok(assignment);
};

export const createWeeklyAllocation = (
  params: CreateWeeklyAllocationParams,
  context: WeeklyAllocationContext,
): Result<WeeklyAllocation, DomainError> => {
  const rangeResult = ensureValidDateRange(params.startDate, params.endDate);
  if (!rangeResult.ok) {
    return rangeResult;
  }

  const weekdays = normalizeWeekdays(params.preferredWeekdays);
  const maxHalfDays = weekdays.length * 2;
  if (params.halfDaysPerWeek < 1 || params.halfDaysPerWeek > maxHalfDays) {
    return err(
      createDomainError('invalid-allocation', 'Allocation exceeds weekly capacity.'),
    );
  }

  return ok({
    id: context.id,
    projectId: params.projectId,
    developerId: params.developerId,
    startDate: params.startDate,
    endDate: params.endDate,
    halfDaysPerWeek: params.halfDaysPerWeek,
    preferredWeekdays: params.preferredWeekdays,
    createdAt: context.createdAt,
  });
};

export const allocationToAssignmentSeeds = (
  allocation: WeeklyAllocation,
): Result<AssignmentSeed[], DomainError> => {
  const weekdays = normalizeWeekdays(allocation.preferredWeekdays);
  const maxHalfDays = weekdays.length * 2;
  if (allocation.halfDaysPerWeek < 1 || allocation.halfDaysPerWeek > maxHalfDays) {
    return err(
      createDomainError('invalid-allocation', 'Allocation exceeds weekly capacity.'),
    );
  }

  const weekStarts = listWeekStartsInRange(allocation.startDate, allocation.endDate);

  return weekStarts.reduce<Result<AssignmentSeed[], DomainError>>(
    (accResult, weekStart) => {
      if (!accResult.ok) {
        return accResult;
      }

      const slots = weekdays.flatMap((weekday) => {
        const date = addDays(weekStart, WEEKDAY_OFFSETS[weekday]);
        if (
          compareIsoDates(date, allocation.startDate) < 0 ||
          compareIsoDates(date, allocation.endDate) > 0
        ) {
          return [];
        }
        return [
          { date, halfDay: 'morning' as const },
          { date, halfDay: 'afternoon' as const },
        ];
      });

      if (slots.length < allocation.halfDaysPerWeek) {
        return err(
          createDomainError(
            'allocation-exceeds-week-capacity',
            'Allocation cannot fit within the requested week.',
          ),
        );
      }

      const weekSeeds: AssignmentSeed[] = slots
        .slice(0, allocation.halfDaysPerWeek)
        .map((slot) => ({
          projectId: allocation.projectId,
          developerId: allocation.developerId,
          date: slot.date,
          halfDay: slot.halfDay,
        }));

      return ok([...accResult.value, ...weekSeeds]);
    },
    ok([]),
  );
};
