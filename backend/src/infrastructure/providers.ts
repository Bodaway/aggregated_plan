import crypto from 'node:crypto';
import type { IdProvider, Clock } from '@application/index';

export const createIdProvider = (): IdProvider => () => crypto.randomUUID();

export const createClock = (): Clock => () => new Date().toISOString().slice(0, 10);
