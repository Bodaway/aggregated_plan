import type { SharePointAdapter } from '@application/sharepoint-adapter';
import type { SharePointConfig } from './sharepoint-config';

export const createGraphSharePointAdapter = (
  config: SharePointConfig,
): SharePointAdapter => ({
  downloadFile: async (graphToken: string) => {
    const url = `https://graph.microsoft.com/v1.0/sites/${config.siteId}/drives/${config.driveId}/root:/${config.filePath}:/content`;

    const response = await fetch(url, {
      headers: {
        Authorization: `Bearer ${graphToken}`,
      },
    });

    if (!response.ok) {
      const errorText = await response.text().catch(() => 'Unknown error');
      throw new Error(
        `SharePoint file download failed (${response.status}): ${errorText}`,
      );
    }

    const arrayBuffer = await response.arrayBuffer();
    return {
      buffer: Buffer.from(arrayBuffer),
      fileName: config.filePath.split('/').pop() ?? 'unknown.xlsx',
    };
  },
});
