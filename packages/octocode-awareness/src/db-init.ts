import { randomUUID } from 'node:crypto';
import {
  assertCanonicalRelationContract,
  assertCanonicalSchemaFingerprint,
  readAwarenessMeta,
} from './db-introspection.js';
import {
  assertDatabaseIntegrity,
  inspectSchemaState,
  SchemaState,
} from './db-introspection.js';
import type { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';
import { withSqliteBusyRetry } from '@octocodeai/agent-contracts/sqlite';
import { AWARENESS_APPLICATION_ID, AWARENESS_SCHEMA_VERSION } from './storage-scope.js';
import { FTS_SCHEMA_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL, SIGNALS_EXPIRES_NOT_NULL_UPGRADE_DDL } from './db-schema.js';
import { hasFts, rebuildFts } from './db-maintenance.js';
import { HISTORY_CAPTURE_DURABILITY_DDL } from './db-history-schema.js';
import { utcNow } from './helpers.js';

function insertAwarenessMeta(db: DatabaseSync): void {
  const now = utcNow();
  db.prepare(`INSERT INTO awareness_meta
      (application_id, schema_version, store_id, created_at, last_migrated_at)
    VALUES (?, ?, ?, ?, ?)`)
    .run(AWARENESS_APPLICATION_ID, AWARENESS_SCHEMA_VERSION, randomUUID(), now, null);
}

export function initDb(db: DatabaseSync, knownState?: SchemaState): void {
  const state = knownState ?? inspectSchemaState(db);
  if (state === 'canonical' || state === 'canonical-path-identity') {
    if (!db.isTransaction) db.exec('PRAGMA foreign_keys = ON');
    assertDatabaseIntegrity(db);
    return;
  }
  if (state === 'schema-generation-upgrade' || state.startsWith('event-envelope-') || state.startsWith('worker-lifecycle-')) {
    throw new Error(`recognized ${state} Awareness store; use database migration preview/apply to create a copy-on-write v${AWARENESS_SCHEMA_VERSION} destination. The source has not been changed.`);
  }
  if (state === 'legacy-renamed-predecessor') {
    throw new Error('recognized legacy-renamed-v1 Awareness store; run a read-only database migration preview. The source has not been changed.');
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
    else if (lockedState === 'signals-expires-not-null-upgrade') {
      // Convert signals.expires_at from nullable TEXT to TEXT NOT NULL via table recreation.
      db.exec(SIGNALS_EXPIRES_NOT_NULL_UPGRADE_DDL);
      assertCanonicalSchemaFingerprint(db);
      assertDatabaseIntegrity(db);
      readAwarenessMeta(db);
    } else if (lockedState === 'history-durability-upgrade') {
      // Absent evidence stays unknown; never backfill a durability claim.
      db.exec(HISTORY_CAPTURE_DURABILITY_DDL);
      assertCanonicalSchemaFingerprint(db);
      assertDatabaseIntegrity(db);
      readAwarenessMeta(db);
    } else if (lockedState === 'history-durability-path-identity-upgrade') {
      // Preserve the path-bound identity. Explicit copy-on-write migration owns metadata.
      db.exec(HISTORY_CAPTURE_DURABILITY_DDL);
      const upgradedState = inspectSchemaState(db);
      if (upgradedState !== 'canonical-path-identity') throw new Error(`unexpected upgraded schema state ${upgradedState}`);
      assertDatabaseIntegrity(db);
    }
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
  insertAwarenessMeta(db);

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
  readAwarenessMeta(db);
}
