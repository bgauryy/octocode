import {
  assertCanonicalRelationContract,
  assertCanonicalSchemaFingerprint,
} from './db-introspection.js';
import {
  assertDatabaseIntegrity,
  AWARENESS_APPLICATION_ID,
  DatabaseSync,
  inspectSchemaState,
  SchemaState,
  withSqliteBusyRetry,
} from './db-runtime.js';
import { FTS_SCHEMA_DDL, HOOK_RECEIPTS_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL } from './db-schema.js';
import { hasFts, rebuildFts } from './db-search.js';

export function initDb(db: DatabaseSync): void {
  initializeDb(db);
}

export function initializeDb(db: DatabaseSync, knownState?: SchemaState): void {
  const state = knownState ?? inspectSchemaState(db);
  if (state === 'canonical') {
    if (!db.isTransaction) db.exec('PRAGMA foreign_keys = ON');
    return;
  }
  if (state === 'prior-hook-receipts') {
    migratePriorHookReceiptSchema(db);
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

export function migratePriorHookReceiptSchema(db: DatabaseSync): void {
  if (db.isTransaction) {
    throw new Error('cannot migrate canonical Awareness inside a caller-owned transaction');
  }
  let began = false;
  try {
    withSqliteBusyRetry(() => db.exec('BEGIN IMMEDIATE'));
    began = true;
    const lockedState = inspectSchemaState(db);
    if (lockedState === 'prior-hook-receipts') db.exec(HOOK_RECEIPTS_DDL);
    else if (lockedState !== 'canonical') throw new Error(`refusing hook receipt migration from schema state ${lockedState}`);
    assertCanonicalRelationContract(db);
    assertCanonicalSchemaFingerprint(db);
    assertDatabaseIntegrity(db);
    db.exec('COMMIT');
    began = false;
  } catch (error) {
    if (began) {
      try { db.exec('ROLLBACK'); } catch { /* transaction already ended */ }
    }
    throw error;
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
