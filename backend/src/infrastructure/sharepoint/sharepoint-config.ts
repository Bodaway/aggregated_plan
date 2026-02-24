export type SharePointConfig = {
  readonly siteId: string;
  readonly driveId: string;
  readonly filePath: string;
};

export const getSharePointConfig = (
  env: Record<string, string | undefined>,
): SharePointConfig => ({
  siteId: env['SHAREPOINT_SITE_ID'] ?? '',
  driveId: env['SHAREPOINT_DRIVE_ID'] ?? '',
  filePath: env['SHAREPOINT_FILE_PATH'] ?? '',
});
