import type { PostgresJsDatabase } from 'drizzle-orm/postgres-js';
import { drizzle } from 'drizzle-orm/postgres-js';
import postgres from 'postgres';
import type { DatabaseConfig } from '../db-config';
import * as schema from '../db/schema';

export type PostgresDatabase = PostgresJsDatabase<typeof schema>;
export type PostgresClient = ReturnType<typeof postgres>;

export const createPostgresClient = (config: DatabaseConfig): PostgresClient => {
  const options = {
    max: config.maxConnections,
    ssl: config.ssl,
  };

  if (config.connectionString) {
    return postgres(config.connectionString, options);
  }

  return postgres({
    host: config.host,
    port: config.port,
    database: config.database,
    username: config.user,
    password: config.password,
    ...options,
  });
};

export const createPostgresDatabase = (client: PostgresClient): PostgresDatabase =>
  drizzle(client, { schema });

export const createPostgresConnection = (config: DatabaseConfig): {
  readonly client: PostgresClient;
  readonly db: PostgresDatabase;
  readonly close: () => Promise<void>;
} => {
  const client = createPostgresClient(config);
  const db = createPostgresDatabase(client);
  return {
    client,
    db,
    close: async () => {
      await client.end({ timeout: 5 });
    },
  };
};
