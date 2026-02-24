import type { EntityId } from './user-types';

export type AuthUser = {
  readonly oid: EntityId;
  readonly displayName: string;
  readonly email: string;
  readonly tenantId: string;
};

export type TokenClaims = {
  readonly oid: string;
  readonly name: string;
  readonly preferred_username: string;
  readonly tid: string;
  readonly sub: string;
  readonly aud: string;
  readonly iss: string;
  readonly exp: number;
  readonly iat: number;
};
