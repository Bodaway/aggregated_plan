import { createRemoteJWKSet, jwtVerify } from 'jose';
import type { MiddlewareHandler } from 'hono';
import type { AuthUser, TokenClaims } from '@aggregated-plan/shared-types';
import type { AuthConfig } from './auth-config';

type AuthEnv = {
  Variables: {
    authUser: AuthUser;
    accessToken: string;
  };
};

const extractBearerToken = (header: string | undefined): string | undefined => {
  if (!header) return undefined;
  const parts = header.split(' ');
  if (parts.length !== 2 || parts[0] !== 'Bearer') return undefined;
  return parts[1];
};

export const createJwtMiddleware = (config: AuthConfig): MiddlewareHandler<AuthEnv> => {
  const jwks = createRemoteJWKSet(new URL(config.jwksUri));

  return async (c, next): Promise<Response | void> => {
    const token = extractBearerToken(c.req.header('Authorization'));
    if (!token) {
      return c.json({ error: 'Missing or invalid Authorization header' }, 401);
    }

    try {
      const { payload } = await jwtVerify(token, jwks, {
        issuer: config.issuer,
        audience: `api://${config.clientId}`,
      });

      const claims = payload as unknown as TokenClaims;

      const authUser: AuthUser = {
        oid: claims.oid,
        displayName: claims.name,
        email: claims.preferred_username,
        tenantId: claims.tid,
      };

      c.set('authUser', authUser);
      c.set('accessToken', token);

      await next();
    } catch {
      return c.json({ error: 'Invalid or expired token' }, 401);
    }
  };
};
