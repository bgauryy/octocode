/**
 * db.ts — the Agent control and session-index SQLite store.
 *
 * The Agent control file, `<home>/agent/agent.sqlite3`, is opened once per process and cached by
 * resolved path. It holds only Agent control and session-index tables. Awareness
 * coordination uses its own workspace- or global-scope database and must never
 * initialise coordination tables on this connection.
 *
 * The low-level `node:sqlite` runtime (warning-filtered `DatabaseSync`, BUSY
 * retry, WAL checkpoint) lives in sqlite.ts; version-gated journal selection in
 * sqlite-version.ts. Requires Node >=22.13.0.
 */
import { resolve } from 'node:path';
import { agentDbPath } from './paths.js';
import { AGENT_APPLICATION_ID, initOctocodeSchema } from './schema.js';
import {
  DatabaseSync,
  SQLITE_BUSY_DEADLINE_MS,
  withSqliteBusyRetry,
} from './sqlite.js';
import { journalModeForSqliteVersion } from './sqlite-version.js';
import { hardenSqliteFiles, preparePrivateSqlitePath } from './permissions.js';

// Cache one connection per resolved path so tests and multiple homes stay
// isolated while the common (single-home) case reuses one handle.
const _cache = new Map<string, DatabaseSync>();

export function assertAgentDatabaseIdentity(db: DatabaseSync): 'fresh' | 'agent' {
  const { application_id: applicationId } = db.prepare('PRAGMA application_id').get() as { application_id: number };
  const relations = db.prepare(`SELECT name FROM sqlite_schema
    WHERE type IN ('table', 'view')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
      AND name NOT GLOB 'memory_fts_*'
    ORDER BY name`).all() as Array<{ name: string }>;
  if (applicationId === AGENT_APPLICATION_ID) return 'agent';
  if (applicationId !== 0) {
    throw new Error(`refusing foreign SQLite application_id ${applicationId}; the agent database requires ${AGENT_APPLICATION_ID}`);
  }
  if (relations.length === 0) return 'fresh';
  throw new Error(`refusing unrecognized application_id=0 database at the agent path; relations: ${relations.map(({ name }) => name).join(', ')}`);
}

/**
 * Open (or reuse) the Agent control DB, apply connection PRAGMAs, and ensure the
 * agent/session schema exists. This is the "init in process running" entry
 * point — call it once at startup; later callers get the cached connection.
 */
export function openOctocodeDb(
  dbPath?: string,
): DatabaseSync {
  const resolved = resolve(dbPath ?? agentDbPath());
  const cached = _cache.get(resolved);
  if (cached) return cached;

  preparePrivateSqlitePath(resolved);
  const db = new DatabaseSync(resolved);
  try {
    // busy_timeout first so the version read can't lose a first-open race.
    db.exec(`PRAGMA busy_timeout = ${SQLITE_BUSY_DEADLINE_MS}`);
    const identity = assertAgentDatabaseIdentity(db);
    const versionRow = db.prepare('SELECT sqlite_version() AS version').get() as { version: string };
    const journalMode = journalModeForSqliteVersion(versionRow.version);
    // journal_mode is a write and may race a first opener → bounded BUSY retry.
    withSqliteBusyRetry(() => db.exec(`PRAGMA journal_mode = ${journalMode}`));
    db.exec('PRAGMA foreign_keys = ON');
    withSqliteBusyRetry(() => db.exec('BEGIN IMMEDIATE'));
    try {
      const lockedIdentity = assertAgentDatabaseIdentity(db);
      if (identity === 'agent' && lockedIdentity !== 'agent') {
        throw new Error('agent database identity changed while opening');
      }
      initOctocodeSchema(db);
      if (lockedIdentity !== 'agent') db.exec(`PRAGMA application_id = ${AGENT_APPLICATION_ID}`);
      db.exec('COMMIT');
    } catch (error) {
      try { db.exec('ROLLBACK'); } catch { /* transaction already ended */ }
      throw error;
    }
    hardenSqliteFiles(resolved);
    _cache.set(resolved, db);
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

/** Close and drop the cached connection for a path (primarily for tests). */
export function closeOctocodeDb(dbPath: string = agentDbPath()): void {
  const resolved = resolve(dbPath);
  const db = _cache.get(resolved);
  if (!db) return;
  try {
    db.close();
  } finally {
    _cache.delete(resolved);
  }
}
