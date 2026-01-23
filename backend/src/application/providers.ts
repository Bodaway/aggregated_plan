import type { EntityId, IsoDateString } from '@aggregated-plan/shared-types';

export type IdProvider = () => EntityId;

export type Clock = () => IsoDateString;
