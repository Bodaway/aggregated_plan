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
  JIRA: '#0052CC',
  EXCEL: '#217346',
  OBSIDIAN: '#7C3AED',
  PERSONAL: '#6B7280',
} as const;

export const SEVERITY_COLORS = {
  CRITICAL: '#DC2626',
  WARNING: '#F59E0B',
  INFORMATION: '#3B82F6',
} as const;
