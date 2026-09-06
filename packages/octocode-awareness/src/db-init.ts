import {
  assertCanonicalRelationContract,
  assertCanonicalSchemaFingerprint,
} from './db-introspection.js';
import {
  assertDatabaseIntegrity,
  inspectSchemaState,
  SchemaState,
} from './db-runtime.js';
import type { DatabaseSync } from '@octocodeai/octocode-shared/sqlite';
import { withSqliteBusyRetry } from '@octocodeai/octocode-shared/sqlite';
import { AWARENESS_APPLICATION_ID } from './storage-scope.js';
import { FTS_SCHEMA_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL } from './db-schema.js';
import { hasFts, rebuildFts } from './db-maintenance.js';

export function initDb(db: DatabaseSync, knownState?: SchemaState): void {
  const state = knownState ?? inspectSchemaState(db);
  if (state === 'canonical') {
    if (!db.isTransaction) db.exec('PRAGMA foreign_keys = ON');
    assertDatabaseIntegrity(db);
    return;
  }
  if (db.isTransaction) {
    throw new Error('cannot initialize canonical Awareness inside a caller-owned transaction');
  }

  db.exec('PRAGMA foreign_keys = OFF');
  let began = false;
  try {
    withSqliteBusyRetry(() => db.exec('BEGIN IMMEDIATE'));
    began = true;
    const lockedState = inspectSchemaState(db);
    if (lockedState === 'fresh') initializeFreshDb(db);
    db.exec('COMMIT');
    began = false;
  } catch (error) {
    if (began) {
      try { db.exec('ROLLBACK'); } catch { /* transaction already ended */ }
    }
    throw error;
  } finally {
    db.exec('PRAGMA foreign_keys = ON');
  }
}

export function initializeFreshDb(db: DatabaseSync): void {
  db.exec(SCHEMA_DDL);
  db.exec(SCHEMA_INDEX_DDL);

  try {
    db.exec(FTS_SCHEMA_DDL);
  } catch {
    /* FTS5 is optional in the embedded SQLite build. */
  }
  if (hasFts(db)) rebuildFts(db);

  assertCanonicalRelationContract(db);
  assertCanonicalSchemaFingerprint(db);
  assertDatabaseIntegrity(db);
  db.exec(`PRAGMA application_id = ${AWARENESS_APPLICATION_ID}`);
}
