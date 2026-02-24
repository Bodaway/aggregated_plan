import { fileURLToPath } from 'node:url';
import { getDatabaseConfig } from '../db-config';
import { applyPendingMigrations } from './migration-runner';
import { createPostgresConnection } from '../postgres/db';

const runMigrations = async (): Promise<void> => {
  const config = getDatabaseConfig(process.env);
  const { db, close } = createPostgresConnection(config);
  try {
    const migrationsFolder = fileURLToPath(new URL('../../../drizzle', import.meta.url));
    await applyPendingMigrations(db, migrationsFolder);
  } finally {
    await close();
  }
};

await runMigrations();
