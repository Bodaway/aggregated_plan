import { ConfidentialClientApplication } from '@azure/msal-node';
import type { AuthConfig } from './auth-config';

export type OboTokenProvider = {
  readonly getGraphToken: (userAccessToken: string) => Promise<string>;
};

export const createOboTokenProvider = (config: AuthConfig): OboTokenProvider => {
  // ConfidentialClientApplication from @azure/msal-node requires `new` —
  // this is a third-party SDK exception to the "no classes" rule.
  const msalClient = new ConfidentialClientApplication({
    auth: {
      clientId: config.clientId,
      authority: `https://login.microsoftonline.com/${config.tenantId}`,
      clientSecret: config.clientSecret,
    },
  });

  const getGraphToken = async (userAccessToken: string): Promise<string> => {
    const result = await msalClient.acquireTokenOnBehalfOf({
      oboAssertion: userAccessToken,
      scopes: ['https://graph.microsoft.com/Files.Read.All'],
    });

    if (!result || !result.accessToken) {
      throw new Error('Failed to acquire Graph API token via OBO flow');
    }

    return result.accessToken;
  };

  return { getGraphToken };
};
