export type ServerConfig = {
  readonly host: string;
  readonly port: number;
};

type ServerConfigEnv = {
  readonly BACKEND_HOST?: string;
  readonly BACKEND_PORT?: string;
};

const DEFAULT_HOST = '127.0.0.1';
const DEFAULT_PORT = 3001;

const parsePort = (value: string | undefined, fallback: number): number => {
  if (value === undefined) {
    return fallback;
  }

  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0 || parsed > 65535) {
    return fallback;
  }

  return parsed;
};

/**
 * Returns server configuration derived from environment variables.
 */
export const getServerConfig = (env: ServerConfigEnv): ServerConfig => ({
  host: env.BACKEND_HOST ?? DEFAULT_HOST,
  port: parsePort(env.BACKEND_PORT, DEFAULT_PORT),
});
