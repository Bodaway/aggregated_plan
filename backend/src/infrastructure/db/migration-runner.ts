import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { sql } from 'drizzle-orm';
import type { PostgresDatabase } from '../postgres/db';

type Migration = {
  readonly id: string;
  readonly sql: string;
};

const ensureMigrationsTable = async (db: PostgresDatabase): Promise<void> => {
  await db.execute(sql`
    CREATE TABLE IF NOT EXISTS schema_migrations (
      id TEXT PRIMARY KEY,
      applied_at TEXT NOT NULL
    )
  `);
};

const loadMigrations = async (migrationsFolder: string): Promise<readonly Migration[]> => {
  const files = (await readdir(migrationsFolder))
    .filter((file) => file.endsWith('.sql'))
    .sort();

  const migrations = await Promise.all(
    files.map(async (file) => ({
      id: file,
      sql: await readFile(path.join(migrationsFolder, file), 'utf8'),
    })),
  );

  return migrations;
};

const listAppliedMigrations = async (db: PostgresDatabase): Promise<ReadonlySet<string>> => {
  const result = await db.execute<{ readonly id: string }>(
    sql`SELECT id FROM schema_migrations`,
  );
  return new Set(result.rows.map((row) => row.id));
};

const applyMigration = async (db: PostgresDatabase, migration: Migration): Promise<void> => {
  await db.execute(sql.raw(migration.sql));
  await db.execute(
    sql`INSERT INTO schema_migrations (id, applied_at) VALUES (${migration.id}, ${new Date().toISOString()})`,
  );
};

export const applyPendingMigrations = async (
  db: PostgresDatabase,
  migrationsFolder: string,
): Promise<void> => {
  await ensureMigrationsTable(db);
  const migrations = await loadMigrations(migrationsFolder);
  const applied = await listAppliedMigrations(db);
  const pending = migrations.filter((migration) => !applied.has(migration.id));

  await pending.reduce(
    (accPromise, migration) => accPromise.then(() => applyMigration(db, migration)),
    Promise.resolve(),
  );
};
