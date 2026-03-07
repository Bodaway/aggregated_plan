import { useState, useCallback } from 'react';
import { useSettings } from '@/hooks/use-settings';
import type { SyncStatusData } from '@/hooks/use-settings';

/** Configuration key constants following the dot-notation naming convention. */
const CONFIG_KEYS = {
  JIRA_BASE_URL: 'jira.base_url',
  JIRA_EMAIL: 'jira.email',
  JIRA_TOKEN: 'jira.token',
  JIRA_PROJECT_KEYS: 'jira.project_keys',
  OUTLOOK_ACCESS_TOKEN: 'outlook.access_token',
  OUTLOOK_CALENDAR_DAYS: 'outlook.calendar_days',
  EXCEL_SHAREPOINT_PATH: 'excel.sharepoint_path',
  EXCEL_SHEET_NAME: 'excel.sheet_name',
  EXCEL_TITLE_COLUMN: 'excel.title_column',
  EXCEL_ASSIGNEE_COLUMN: 'excel.assignee_column',
  EXCEL_PROJECT_COLUMN: 'excel.project_column',
  EXCEL_DATE_COLUMN: 'excel.date_column',
  EXCEL_JIRA_KEY_COLUMN: 'excel.jira_key_column',
  GENERAL_WORKING_HOURS: 'general.working_hours',
  GENERAL_CAPACITY: 'general.capacity',
} as const;

/** Chevron icon for collapsible sections. */
function ChevronIcon({ expanded }: { readonly expanded: boolean }) {
  return (
    <svg
      className={`w-5 h-5 text-gray-500 transition-transform duration-200 ${
        expanded ? 'rotate-180' : ''
      }`}
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={2}
    >
      <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 8.25l-7.5 7.5-7.5-7.5" />
    </svg>
  );
}

/** Reusable collapsible card section. */
function SettingsSection({
  title,
  icon,
  children,
  defaultOpen = false,
}: {
  readonly title: string;
  readonly icon: React.ReactNode;
  readonly children: React.ReactNode;
  readonly defaultOpen?: boolean;
}) {
  const [expanded, setExpanded] = useState(defaultOpen);

  return (
    <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded(prev => !prev)}
        className="w-full flex items-center justify-between px-5 py-4 hover:bg-gray-50 transition-colors"
      >
        <div className="flex items-center gap-3">
          {icon}
          <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">{title}</h3>
        </div>
        <ChevronIcon expanded={expanded} />
      </button>
      {expanded && <div className="px-5 pb-5 border-t border-gray-100 pt-4">{children}</div>}
    </div>
  );
}

/** Reusable text input field. */
function SettingsInput({
  label,
  value,
  onChange,
  type = 'text',
  placeholder,
  description,
}: {
  readonly label: string;
  readonly value: string;
  readonly onChange: (value: string) => void;
  readonly type?: 'text' | 'password' | 'number';
  readonly placeholder?: string;
  readonly description?: string;
}) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-700 mb-1">{label}</label>
      {description && <p className="text-xs text-gray-400 mb-1">{description}</p>}
      <input
        type={type}
        value={value}
        onChange={e => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 transition-colors"
      />
    </div>
  );
}

/** Save button for a section. */
function SaveButton({
  onClick,
  saving,
  disabled,
}: {
  readonly onClick: () => void;
  readonly saving: boolean;
  readonly disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={saving || disabled}
      className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
    >
      {saving && (
        <div className="w-3.5 h-3.5 border-2 border-white border-t-transparent rounded-full animate-spin" />
      )}
      {saving ? 'Saving...' : 'Save'}
    </button>
  );
}

/** Jira configuration icon. */
function JiraIcon() {
  return (
    <svg className="w-5 h-5 text-blue-600" fill="currentColor" viewBox="0 0 24 24">
      <path d="M11.53 2c0 2.4 1.97 4.35 4.35 4.35h1.78v1.7c0 2.4 1.94 4.34 4.34 4.35V2.84a.84.84 0 00-.84-.84H11.53zM6.77 6.8a4.36 4.36 0 004.34 4.34h1.78v1.72a4.36 4.36 0 004.34 4.34V7.63a.84.84 0 00-.83-.83H6.77zM2 11.6c0 2.4 1.95 4.34 4.35 4.34h1.78v1.72c0 2.4 1.94 4.34 4.35 4.34v-9.57a.84.84 0 00-.84-.83H2z" />
    </svg>
  );
}

/** Microsoft/Outlook icon. */
function OutlookIcon() {
  return (
    <svg className="w-5 h-5 text-blue-500" fill="currentColor" viewBox="0 0 24 24">
      <path d="M7.88 12.04q0 .45-.11.87-.1.41-.33.74-.22.33-.58.52-.35.2-.8.2-.44 0-.78-.2-.34-.19-.56-.52-.22-.33-.33-.74-.1-.42-.1-.87t.1-.87q.1-.43.33-.76.22-.33.56-.52.34-.2.78-.2.45 0 .8.2.36.19.58.52.23.33.33.76.1.43.1.87zm-5.66-.46Q2.22 13 2.7 14.1q.5 1.1 1.41 1.83.92.73 2.16 1.14 1.24.4 2.74.4 1.46 0 2.68-.36 1.23-.36 2.12-1.06.9-.7 1.4-1.74.5-1.04.5-2.4 0-1.36-.5-2.38-.5-1.03-1.4-1.73-.89-.7-2.12-1.08Q10.47 6.36 9.01 6.36q-1.5 0-2.74.42-1.24.42-2.16 1.16-.91.74-1.41 1.8-.49 1.06-.49 2.38l.01-.54zM21.2 4.5H10.9l-.1 3.2h4.15v12.8h2.75V7.7h3.5V4.5z" />
    </svg>
  );
}

/** Excel/SharePoint icon. */
function ExcelIcon() {
  return (
    <svg className="w-5 h-5 text-green-600" fill="currentColor" viewBox="0 0 24 24">
      <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6zM6 20V4h7v5h5v11H6zm2-6l2.5 3.5L13 14h1.5l-3.5 5 3.5 5H13l-2.5-3.5L8 24H6.5l3.5-5-3.5-5H8z" />
    </svg>
  );
}

/** Sync status icon. */
function SyncIcon() {
  return (
    <svg
      className="w-5 h-5 text-indigo-600"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={2}
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182M2.985 19.644l3.181-3.182"
      />
    </svg>
  );
}

/** General settings icon. */
function GearIcon() {
  return (
    <svg
      className="w-5 h-5 text-gray-600"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={2}
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.24-.438.613-.431.992a6.759 6.759 0 010 .255c-.007.378.138.75.43.99l1.005.828c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.28c-.09.543-.56.941-1.11.941h-2.594c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.992a6.932 6.932 0 010-.255c.007-.378-.138-.75-.43-.99l-1.004-.828a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.281z"
      />
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
      />
    </svg>
  );
}

/** Get a human-readable status badge. */
function getStatusBadge(status: string) {
  switch (status) {
    case 'SUCCESS':
    case 'SYNCED':
      return { color: 'bg-green-100 text-green-700', label: 'Synced' };
    case 'SYNCING':
    case 'IN_PROGRESS':
      return { color: 'bg-yellow-100 text-yellow-700', label: 'Syncing...' };
    case 'ERROR':
    case 'FAILED':
      return { color: 'bg-red-100 text-red-700', label: 'Error' };
    case 'IDLE':
    default:
      return { color: 'bg-gray-100 text-gray-600', label: 'Idle' };
  }
}

/** Format a timestamp for display. */
function formatTimestamp(ts: string | null): string {
  if (!ts) return 'Never';
  try {
    const d = new Date(ts);
    return d.toLocaleString([], {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return ts;
  }
}

/** Sync status row for a single source. */
function SyncStatusRow({
  status,
  onSync,
  syncing,
}: {
  readonly status: SyncStatusData;
  readonly onSync: () => void;
  readonly syncing: boolean;
}) {
  const badge = getStatusBadge(status.status);

  return (
    <div className="flex items-center justify-between py-3 border-b border-gray-100 last:border-b-0">
      <div className="flex items-center gap-3">
        <span className="text-sm font-medium text-gray-700 w-16">{status.source}</span>
        <span
          className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${badge.color}`}
        >
          {badge.label}
        </span>
        <span className="text-xs text-gray-400">
          Last sync: {formatTimestamp(status.lastSyncAt)}
        </span>
      </div>
      <div className="flex items-center gap-2">
        {status.errorMessage && (
          <span className="text-xs text-red-500 max-w-xs truncate" title={status.errorMessage}>
            {status.errorMessage}
          </span>
        )}
        <button
          type="button"
          onClick={onSync}
          disabled={syncing}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-indigo-600 border border-indigo-300 rounded-md hover:bg-indigo-50 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          {syncing ? (
            <div className="w-3 h-3 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin" />
          ) : (
            <svg
              className="w-3 h-3"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182M2.985 19.644l3.181-3.182"
              />
            </svg>
          )}
          Sync Now
        </button>
      </div>
    </div>
  );
}

export function SettingsPage() {
  const { configuration, syncStatuses, loading, error, syncing, updateConfig, forceSync } =
    useSettings();

  // Local state for form fields (initialized from fetched configuration)
  const [localConfig, setLocalConfig] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [syncingSource, setSyncingSource] = useState<string | null>(null);

  // Merge fetched config with local overrides
  const getConfigValue = useCallback(
    (key: string, fallback = ''): string => {
      if (key in localConfig) return localConfig[key];
      const fetched = configuration[key];
      return typeof fetched === 'string' ? fetched : fallback;
    },
    [localConfig, configuration]
  );

  const setConfigValue = useCallback((key: string, value: string) => {
    setLocalConfig(prev => ({ ...prev, [key]: value }));
    setSaveMessage(null);
  }, []);

  /** Save a group of config keys to the backend. */
  const saveConfigKeys = useCallback(
    async (keys: readonly string[]) => {
      setSaving(true);
      setSaveMessage(null);
      try {
        for (const key of keys) {
          const value = getConfigValue(key);
          await updateConfig(key, value);
        }
        setSaveMessage('Settings saved successfully.');
        // Clear local overrides for saved keys
        setLocalConfig(prev => {
          const next = { ...prev };
          keys.forEach(k => {
            delete next[k];
          });
          return next;
        });
      } catch {
        setSaveMessage('Failed to save settings.');
      } finally {
        setSaving(false);
      }
    },
    [getConfigValue, updateConfig]
  );

  const handleForceSync = useCallback(
    async (source: string) => {
      setSyncingSource(source);
      try {
        await forceSync(source);
      } finally {
        setSyncingSource(null);
      }
    },
    [forceSync]
  );

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <p className="text-red-500 text-sm font-medium">Failed to load settings</p>
          <p className="text-gray-400 text-xs mt-1">{error.message}</p>
        </div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <div className="w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin mx-auto mb-2" />
          <p className="text-gray-500 text-sm">Loading settings...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4 max-w-4xl">
      {/* Success/error message */}
      {saveMessage && (
        <div
          className={`px-4 py-2 rounded-md text-sm ${
            saveMessage.includes('success')
              ? 'bg-green-50 text-green-700 border border-green-200'
              : 'bg-red-50 text-red-700 border border-red-200'
          }`}
        >
          {saveMessage}
        </div>
      )}

      {/* Section 1: Jira Configuration */}
      <SettingsSection title="Jira Configuration" icon={<JiraIcon />} defaultOpen>
        <div className="space-y-4">
          <SettingsInput
            label="Base URL"
            value={getConfigValue(CONFIG_KEYS.JIRA_BASE_URL)}
            onChange={v => setConfigValue(CONFIG_KEYS.JIRA_BASE_URL, v)}
            placeholder="https://your-domain.atlassian.net"
            description="Your Jira Cloud instance URL"
          />
          <SettingsInput
            label="Email"
            value={getConfigValue(CONFIG_KEYS.JIRA_EMAIL)}
            onChange={v => setConfigValue(CONFIG_KEYS.JIRA_EMAIL, v)}
            placeholder="user@example.com"
            description="Email address associated with your Jira account"
          />
          <SettingsInput
            label="API Token"
            value={getConfigValue(CONFIG_KEYS.JIRA_TOKEN)}
            onChange={v => setConfigValue(CONFIG_KEYS.JIRA_TOKEN, v)}
            type="password"
            placeholder="Your Jira API token"
            description="Generate one at id.atlassian.com/manage-profile/security/api-tokens"
          />
          <SettingsInput
            label="Project Keys"
            value={getConfigValue(CONFIG_KEYS.JIRA_PROJECT_KEYS)}
            onChange={v => setConfigValue(CONFIG_KEYS.JIRA_PROJECT_KEYS, v)}
            placeholder="PROJ1, PROJ2, PROJ3"
            description="Comma-separated list of Jira project keys to sync"
          />

          <div className="flex items-center gap-3 pt-2">
            <SaveButton
              onClick={() =>
                saveConfigKeys([
                  CONFIG_KEYS.JIRA_BASE_URL,
                  CONFIG_KEYS.JIRA_EMAIL,
                  CONFIG_KEYS.JIRA_TOKEN,
                  CONFIG_KEYS.JIRA_PROJECT_KEYS,
                ])
              }
              saving={saving}
            />
            <button
              type="button"
              onClick={() => handleForceSync('JIRA')}
              disabled={syncing || syncingSource === 'JIRA'}
              className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-indigo-600 border border-indigo-300 rounded-md hover:bg-indigo-50 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {syncingSource === 'JIRA' && (
                <div className="w-3.5 h-3.5 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin" />
              )}
              Test Connection
            </button>
          </div>
        </div>
      </SettingsSection>

      {/* Section 2: Microsoft Graph (Outlook + Excel) */}
      <SettingsSection title="Microsoft Graph (Outlook)" icon={<OutlookIcon />}>
        <div className="space-y-4">
          <SettingsInput
            label="Access Token"
            value={getConfigValue(CONFIG_KEYS.OUTLOOK_ACCESS_TOKEN)}
            onChange={v => setConfigValue(CONFIG_KEYS.OUTLOOK_ACCESS_TOKEN, v)}
            type="password"
            placeholder="Microsoft Graph access token"
            description="OAuth2 access token for Microsoft Graph API"
          />
          <SettingsInput
            label="Calendar Range (days)"
            value={getConfigValue(CONFIG_KEYS.OUTLOOK_CALENDAR_DAYS, '14')}
            onChange={v => setConfigValue(CONFIG_KEYS.OUTLOOK_CALENDAR_DAYS, v)}
            type="number"
            placeholder="14"
            description="Number of days ahead to sync calendar events"
          />

          <div className="pt-2">
            <SaveButton
              onClick={() =>
                saveConfigKeys([
                  CONFIG_KEYS.OUTLOOK_ACCESS_TOKEN,
                  CONFIG_KEYS.OUTLOOK_CALENDAR_DAYS,
                ])
              }
              saving={saving}
            />
          </div>
        </div>
      </SettingsSection>

      {/* Section 3: Excel/SharePoint Configuration */}
      <SettingsSection title="Excel / SharePoint" icon={<ExcelIcon />}>
        <div className="space-y-4">
          <SettingsInput
            label="SharePoint File Path"
            value={getConfigValue(CONFIG_KEYS.EXCEL_SHAREPOINT_PATH)}
            onChange={v => setConfigValue(CONFIG_KEYS.EXCEL_SHAREPOINT_PATH, v)}
            placeholder="/sites/MySite/Shared Documents/planning.xlsx"
            description="Path to the Excel file in SharePoint"
          />
          <SettingsInput
            label="Sheet Name"
            value={getConfigValue(CONFIG_KEYS.EXCEL_SHEET_NAME)}
            onChange={v => setConfigValue(CONFIG_KEYS.EXCEL_SHEET_NAME, v)}
            placeholder="Sheet1"
            description="Name of the worksheet to read"
          />

          {/* Column mappings */}
          <div>
            <h4 className="text-sm font-medium text-gray-600 mb-3">Column Mappings</h4>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <SettingsInput
                label="Title Column"
                value={getConfigValue(CONFIG_KEYS.EXCEL_TITLE_COLUMN, 'Title')}
                onChange={v => setConfigValue(CONFIG_KEYS.EXCEL_TITLE_COLUMN, v)}
                placeholder="Title"
              />
              <SettingsInput
                label="Assignee Column"
                value={getConfigValue(CONFIG_KEYS.EXCEL_ASSIGNEE_COLUMN, 'Assignee')}
                onChange={v => setConfigValue(CONFIG_KEYS.EXCEL_ASSIGNEE_COLUMN, v)}
                placeholder="Assignee"
              />
              <SettingsInput
                label="Project Column"
                value={getConfigValue(CONFIG_KEYS.EXCEL_PROJECT_COLUMN, 'Project')}
                onChange={v => setConfigValue(CONFIG_KEYS.EXCEL_PROJECT_COLUMN, v)}
                placeholder="Project"
              />
              <SettingsInput
                label="Date Column"
                value={getConfigValue(CONFIG_KEYS.EXCEL_DATE_COLUMN, 'Date')}
                onChange={v => setConfigValue(CONFIG_KEYS.EXCEL_DATE_COLUMN, v)}
                placeholder="Date"
              />
              <SettingsInput
                label="Jira Key Column"
                value={getConfigValue(CONFIG_KEYS.EXCEL_JIRA_KEY_COLUMN, 'JiraKey')}
                onChange={v => setConfigValue(CONFIG_KEYS.EXCEL_JIRA_KEY_COLUMN, v)}
                placeholder="JiraKey"
              />
            </div>
          </div>

          <div className="pt-2">
            <SaveButton
              onClick={() =>
                saveConfigKeys([
                  CONFIG_KEYS.EXCEL_SHAREPOINT_PATH,
                  CONFIG_KEYS.EXCEL_SHEET_NAME,
                  CONFIG_KEYS.EXCEL_TITLE_COLUMN,
                  CONFIG_KEYS.EXCEL_ASSIGNEE_COLUMN,
                  CONFIG_KEYS.EXCEL_PROJECT_COLUMN,
                  CONFIG_KEYS.EXCEL_DATE_COLUMN,
                  CONFIG_KEYS.EXCEL_JIRA_KEY_COLUMN,
                ])
              }
              saving={saving}
            />
          </div>
        </div>
      </SettingsSection>

      {/* Section 4: Sync Status */}
      <SettingsSection title="Sync Status" icon={<SyncIcon />} defaultOpen>
        <div>
          {syncStatuses.length > 0 ? (
            <div>
              {syncStatuses.map(status => (
                <SyncStatusRow
                  key={status.source}
                  status={status}
                  onSync={() => handleForceSync(status.source)}
                  syncing={syncingSource === status.source}
                />
              ))}
            </div>
          ) : (
            <div className="text-center py-6">
              <p className="text-sm text-gray-500">No sync sources configured yet.</p>
              <p className="text-xs text-gray-400 mt-1">
                Configure Jira, Outlook, or Excel above to see sync status.
              </p>
            </div>
          )}

          {/* Sync All button */}
          <div className="mt-4 pt-4 border-t border-gray-100">
            <button
              type="button"
              onClick={() => handleForceSync('ALL')}
              disabled={syncing || syncingSource !== null}
              className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-indigo-600 rounded-md hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {syncingSource === 'ALL' && (
                <div className="w-3.5 h-3.5 border-2 border-white border-t-transparent rounded-full animate-spin" />
              )}
              Sync All Sources
            </button>
          </div>
        </div>
      </SettingsSection>

      {/* Section 5: General Settings */}
      <SettingsSection title="General" icon={<GearIcon />}>
        <div className="space-y-4">
          <SettingsInput
            label="Working Hours per Day"
            value={getConfigValue(CONFIG_KEYS.GENERAL_WORKING_HOURS, '8')}
            onChange={v => setConfigValue(CONFIG_KEYS.GENERAL_WORKING_HOURS, v)}
            type="number"
            placeholder="8"
            description="Standard number of working hours in a day"
          />
          <SettingsInput
            label="Default Capacity (half-days per week)"
            value={getConfigValue(CONFIG_KEYS.GENERAL_CAPACITY, '10')}
            onChange={v => setConfigValue(CONFIG_KEYS.GENERAL_CAPACITY, v)}
            type="number"
            placeholder="10"
            description="Default number of half-day slots available per week (max 10)"
          />

          <div className="pt-2">
            <SaveButton
              onClick={() =>
                saveConfigKeys([
                  CONFIG_KEYS.GENERAL_WORKING_HOURS,
                  CONFIG_KEYS.GENERAL_CAPACITY,
                ])
              }
              saving={saving}
            />
          </div>
        </div>
      </SettingsSection>
    </div>
  );
}
