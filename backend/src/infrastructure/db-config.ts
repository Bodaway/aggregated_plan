export const APP_ENVS = ['local', 'dev', 'test', 'staging', 'prod'] as const;
export type AppEnv = (typeof APP_ENVS)[number];

export const PERSISTENCE_KINDS = ['in-memory', 'postgres'] as const;
export type PersistenceKind = (typeof PERSISTENCE_KINDS)[number];

export type DatabaseConfig = {
  readonly connectionString?: string;
  readonly host: string;
  readonly port: number;
  readonly database: string;
  readonly user: string;
  readonly password: string;
  readonly ssl: boolean;
  readonly maxConnections: number;
};

type DbConfigEnv = {
  readonly APP_ENV?: string;
  readonly PERSISTENCE_KIND?: string;
  readonly DATABASE_URL?: string;
  readonly DB_HOST?: string;
  readonly DB_PORT?: string;
  readonly DB_NAME?: string;
  readonly DB_USER?: string;
  readonly DB_PASSWORD?: string;
  readonly DB_SSL?: string;
  readonly DB_MAX_CONNECTIONS?: string;
};

const DEFAULT_APP_ENV: AppEnv = 'local';
const DEFAULT_PERSISTENCE_KIND: PersistenceKind = 'in-memory';
const DEFAULT_DB_HOST = '127.0.0.1';
const DEFAULT_DB_PORT = 5432;
const DEFAULT_DB_NAME = 'aggregated_plan';
const DEFAULT_DB_USER = 'postgres';
const DEFAULT_DB_PASSWORD = 'postgres';
const DEFAULT_DB_SSL = false;
const DEFAULT_DB_MAX_CONNECTIONS = 10;

const isAppEnv = (value: string): value is AppEnv =>
  APP_ENVS.some((env) => env === value);

const isPersistenceKind = (value: string): value is PersistenceKind =>
  PERSISTENCE_KINDS.some((kind) => kind === value);

const parseNumber = (value: string | undefined, fallback: number): number => {
  if (value === undefined) {
    return fallback;
  }
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return fallback;
  }
  return parsed;
};

const parseBoolean = (value: string | undefined, fallback: boolean): boolean => {
  if (value === undefined) {
    return fallback;
  }
  const normalized = value.trim().toLowerCase();
  if (['true', '1', 'yes', 'on'].includes(normalized)) {
    return true;
  }
  if (['false', '0', 'no', 'off'].includes(normalized)) {
    return false;
  }
  return fallback;
};

export const getAppEnv = (env: DbConfigEnv): AppEnv => {
  if (env.APP_ENV && isAppEnv(env.APP_ENV)) {
    return env.APP_ENV;
  }
  return DEFAULT_APP_ENV;
};

export const getPersistenceKind = (env: DbConfigEnv): PersistenceKind => {
  if (env.PERSISTENCE_KIND && isPersistenceKind(env.PERSISTENCE_KIND)) {
    return env.PERSISTENCE_KIND;
  }
  return DEFAULT_PERSISTENCE_KIND;
};

export const getDatabaseConfig = (env: DbConfigEnv): DatabaseConfig => ({
  connectionString: env.DATABASE_URL,
  host: env.DB_HOST ?? DEFAULT_DB_HOST,
  port: parseNumber(env.DB_PORT, DEFAULT_DB_PORT),
  database: env.DB_NAME ?? DEFAULT_DB_NAME,
  user: env.DB_USER ?? DEFAULT_DB_USER,
  password: env.DB_PASSWORD ?? DEFAULT_DB_PASSWORD,
  ssl: parseBoolean(env.DB_SSL, DEFAULT_DB_SSL),
  maxConnections: parseNumber(env.DB_MAX_CONNECTIONS, DEFAULT_DB_MAX_CONNECTIONS),
});
