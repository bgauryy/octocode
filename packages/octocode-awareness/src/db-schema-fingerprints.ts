/** Private fingerprint-assertion helpers used by db-introspection. Not exported from the package index. */
import { readSchemaObjects, assertSchemaObjects } from './agent-store-schema.js';
import { DatabaseSync } from './sqlite.js';
import { FTS_SCHEMA_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL } from './db-schema.js';
import { WORKER_LIFECYCLE_DDL } from './db-worker-schema.js';
import { EVENT_OUTBOX_V1_DDL, EVENT_OUTBOX_V1_INDEX_DDL } from './db-continuity-schema.js';
import { PREDECESSOR_REFINEMENTS_DDL } from './db-predecessor-schema.js';

export function assertSchemaFingerprint(db: DatabaseSync, options: {
  omittedTables?: readonly string[];
  eventOutboxVersion?: 1 | 2;
  includeRefinements?: boolean;
} = {}): void {
  const objects = readSchemaObjects(db);
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(SCHEMA_DDL);
    canonical.exec(SCHEMA_INDEX_DDL);
    if (options.includeRefinements) canonical.exec(PREDECESSOR_REFINEMENTS_DDL);
    if (options.eventOutboxVersion === 1) {
      canonical.exec('DROP INDEX IF EXISTS idx_event_outbox_retention_sequence');
      canonical.exec('DROP INDEX IF EXISTS idx_event_outbox_type_sequence');
      canonical.exec('DROP TABLE event_outbox');
      canonical.exec(EVENT_OUTBOX_V1_DDL);
      canonical.exec(EVENT_OUTBOX_V1_INDEX_DDL);
    }
    for (const table of options.omittedTables ?? []) canonical.exec(`DROP TABLE ${JSON.stringify(table)}`);
    if (objects.some(({ name }) => name === 'memories_fts')) canonical.exec(FTS_SCHEMA_DDL);
    if (objects.some(({ name }) => name === 'worker_lifecycle_events')) canonical.exec(WORKER_LIFECYCLE_DDL);
    assertSchemaObjects(objects, readSchemaObjects(canonical));
  } finally {
    canonical.close();
  }
}

export function assertMessageRetentionPredecessorFingerprint(db: DatabaseSync): void {
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(SCHEMA_DDL);
    canonical.exec(SCHEMA_INDEX_DDL);
    canonical.exec('DROP INDEX IF EXISTS idx_signals_expires_at');
    canonical.exec('ALTER TABLE signals DROP COLUMN expires_at');
    if (readSchemaObjects(db).some(({ name }) => name === 'memories_fts')) canonical.exec(FTS_SCHEMA_DDL);
    if (readSchemaObjects(db).some(({ name }) => name === 'worker_lifecycle_events')) canonical.exec(WORKER_LIFECYCLE_DDL);
    assertSchemaObjects(readSchemaObjects(db), readSchemaObjects(canonical));
  } finally {
    canonical.close();
  }
}

/** Predecessor fingerprint for databases with signals.expires_at as nullable TEXT (no NOT NULL). */
export function assertSignalsNullableExpiresPredecessorFingerprint(db: DatabaseSync): void {
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(SCHEMA_DDL);
    canonical.exec(SCHEMA_INDEX_DDL);
    // Downgrade expires_at to nullable to match the intermediate v5 DDL.
    canonical.exec('DROP INDEX IF EXISTS idx_signals_expires_at');
    canonical.exec('ALTER TABLE signals DROP COLUMN expires_at');
    canonical.exec('ALTER TABLE signals ADD COLUMN expires_at TEXT');
    canonical.exec('CREATE INDEX idx_signals_expires_at ON signals(expires_at)');
    if (readSchemaObjects(db).some(({ name }) => name === 'memories_fts')) canonical.exec(FTS_SCHEMA_DDL);
    if (readSchemaObjects(db).some(({ name }) => name === 'worker_lifecycle_events')) canonical.exec(WORKER_LIFECYCLE_DDL);
    assertSchemaObjects(readSchemaObjects(db), readSchemaObjects(canonical));
  } finally {
    canonical.close();
  }
}
