export const HALF_DAY_HOURS = 4;
export const FULL_DAY_HOURS = 8;
export const DEFAULT_CAPACITY_HALF_DAYS = 10; // per week

export const QUADRANT_LABELS = {
  UrgentImportant: 'Do First',
  Important: 'Schedule',
  Urgent: 'Delegate',
  Neither: 'Eliminate',
} as const;

export const SOURCE_COLORS = {
  JIRA: 'var(--cn-blue)',
  EXCEL: 'var(--cn-green)',
  OBSIDIAN: 'var(--cn-purple)',
  PERSONAL: 'var(--app-ink-mid)',
} as const;

export const SEVERITY_COLORS = {
  CRITICAL: 'var(--cn-red)',
  WARNING: 'var(--cn-yellow)',
  INFORMATION: 'var(--cn-teal)',
} as const;

/**
 * Directory `importMemories` reads by default — the harness memory folder.
 * Override with `VITE_MEMORY_IMPORT_DIR`. The BACKEND resolves this path on its
 * own filesystem, so it must be absolute: nothing expands a leading `~`.
 */
export const MEMORY_IMPORT_DEFAULT_DIR =
  import.meta.env.VITE_MEMORY_IMPORT_DIR?.toString() ??
  '/home/mbt/.claude/projects/-home-mbt-appfactory-aggregated-plan/memory';
