import type { IFuseOptions } from 'fuse.js';
import type { SearchableTask } from './types';

export const FUSE_OPTIONS: IFuseOptions<SearchableTask> = {
  keys: [
    { name: 'title',       weight: 0.40 },
    { name: 'sourceId',    weight: 0.25 },
    { name: 'tags',        weight: 0.15 },
    { name: 'projectName', weight: 0.08 },
    { name: 'assignee',    weight: 0.07 },
    { name: 'description', weight: 0.05 },
  ],
  threshold: 0.35,
  ignoreLocation: true,
  includeMatches: true,
  minMatchCharLength: 2,
};

export const MAX_MATCHES = 50;
export const MAX_DROPDOWN_ROWS = 8;
export const MIN_QUERY_LENGTH = 2;
