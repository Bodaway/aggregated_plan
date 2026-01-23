import type { IsoDateString } from './time-types';

export type EntityId = string;

export type UserRole = 'admin' | 'developer' | 'viewer';

export type User = {
  readonly id: EntityId;
  readonly displayName: string;
  readonly email: string;
  readonly role: UserRole;
  readonly createdAt: IsoDateString;
  readonly updatedAt: IsoDateString;
};
