export type IsoDateString = `${number}-${number}-${number}`;

export type Weekday =
  | 'monday'
  | 'tuesday'
  | 'wednesday'
  | 'thursday'
  | 'friday'
  | 'saturday'
  | 'sunday';

export type HalfDay = 'morning' | 'afternoon';

export type DateRange = {
  readonly startDate: IsoDateString;
  readonly endDate: IsoDateString;
};
