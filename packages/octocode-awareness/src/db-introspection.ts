import { createHash } from 'node:crypto';
import { realpathSync } from 'node:fs';
import { AGENT_APPLICATION_ID, readSchemaObjects, assertSchemaObjects } from '@octocodeai/agent-contracts/schema';
import {
  AWARENESS_APPLICATION_ID,
  AWARENESS_MIGRATABLE_SCHEMA_VERSIONS,
  AWARENESS_SCHEMA_VERSION,
} from './storage-scope.js';
import type { TableInfoRow } from './types/work-maintenance.js';
import { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';
import { FTS_SCHEMA_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL } from './db-schema.js';
import { WORKER_LIFECYCLE_DDL } from './db-worker-schema.js';
import { EVENT_OUTBOX_V1_DDL, EVENT_OUTBOX_V1_INDEX_DDL } from './db-continuity-schema.js';
import {
  LEGACY_RENAMED_V1_SCHEMA_DDL,
  PREDECESSOR_EVENT_RELATIONS,
  PREDECESSOR_EVENT_RELATIONS_DDL,
  PREDECESSOR_REFINEMENTS_DDL,
} from './db-predecessor-schema.js';

export function tableColumns(db: DatabaseSync, tableName: string): Set<string> {
  const rows = db.prepare(`PRAGMA table_info(${tableName})`).all() as unknown as TableInfoRow[];
  return new Set(rows.map((row) => row.name));
}

export interface ColumnInfo {
  name: string;
  type: string;
  notnull: number;
  dflt_value: string | null;
}

let _canonicalColumns: Map<string, ColumnInfo[]> | undefined;

/** Desired columns per table, derived from the executable DDL. */
export function canonicalColumns(): Map<string, ColumnInfo[]> {
  if (_canonicalColumns) return _canonicalColumns;
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(SCHEMA_DDL);
    const tables = canonical.prepare(
      "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    ).all() as unknown as Array<{ name: string }>;
    _canonicalColumns = new Map(tables.map(({ name }) => [
      name,
      canonical.prepare(`PRAGMA table_info(${name})`).all() as unknown as ColumnInfo[],
    ]));
    return _canonicalColumns;
  } finally {
    canonical.close();
  }
}

function isEventStreamConvergencePredecessor(db: DatabaseSync, identity: SchemaIdentity): boolean {
  if (identity.applicationId !== AWARENESS_APPLICATION_ID) return false;
  const current = new Set(canonicalColumns().keys());
  const hasRefinements = identity.relations.some(({ name }) => name === 'refinements');
  const expected = new Set([...current, ...(hasRefinements ? ['refinements'] : []), ...PREDECESSOR_EVENT_RELATIONS]);
  const actual = identity.relations.filter(({ name }) => !/^memories_fts(?:_|$)/.test(name));
  if (actual.length !== expected.size || actual.some(({ name, type }) => type !== 'table' || !expected.has(name))) return false;
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(SCHEMA_DDL);
    canonical.exec(SCHEMA_INDEX_DDL);
    if (hasRefinements) canonical.exec(PREDECESSOR_REFINEMENTS_DDL);
    canonical.exec(PREDECESSOR_EVENT_RELATIONS_DDL);
    if (identity.relations.some(({ name }) => name === 'memories_fts')) canonical.exec(FTS_SCHEMA_DDL);
    assertSchemaObjects(readSchemaObjects(db), readSchemaObjects(canonical));
    readAwarenessMeta(db);
    return true;
  } finally {
    canonical.close();
  }
}

export function assertCanonicalRelationContract(
  db: DatabaseSync,
  relations?: SchemaIdentity['relations'],
): void {
  const actualRows = relations ?? readSchemaIdentity(db).relations;
  const expected = new Set(canonicalColumns().keys());
  const actual = new Set(actualRows.map(({ name }) => name));
  const missing = [...expected].filter((name) => !actual.has(name));
  const unexpected = actualRows.filter(({ name, type }) => (
    type !== 'table' || (!expected.has(name) && name !== 'memories_fts' && name !== 'worker_lifecycle_events')
  ));
  if (missing.length === 0 && unexpected.length === 0) return;
  const details = [
    missing.length > 0 ? `missing: ${missing.join(', ')}` : null,
    unexpected.length > 0 ? `unexpected: ${unexpected.map(({ name }) => name).join(', ')}` : null,
  ].filter((value): value is string => value !== null).join('; ');
  throw new Error(`canonical relation contract mismatch (${details})`);
}

export function assertCanonicalSchemaFingerprint(db: DatabaseSync): void {
  assertSchemaFingerprint(db);
}

function assertSchemaFingerprint(db: DatabaseSync, options: {
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

export interface SchemaIdentity {
  applicationId: number;
  relations: Array<{ name: string; type: string }>;
}

export interface AwarenessMeta {
  applicationId: number;
  schemaVersion: number;
  storeId: string;
  createdAt: string;
  lastMigratedAt: string | null;
}

export type SchemaState =
  | 'fresh'
  | 'canonical'
  | 'canonical-path-identity'
  | 'schema-generation-upgrade'
  | 'history-durability-upgrade'
  | 'history-durability-path-identity-upgrade'
  | 'event-envelope-upgrade'
  | 'event-envelope-path-identity-upgrade'
  | 'event-envelope-history-durability-upgrade'
  | 'event-envelope-history-durability-path-identity-upgrade'
  | 'worker-lifecycle-upgrade'
  | 'worker-lifecycle-path-identity-upgrade'
  | 'worker-lifecycle-history-durability-upgrade'
  | 'worker-lifecycle-history-durability-path-identity-upgrade'
  | 'refinements-upgrade'
  | 'event-stream-convergence-upgrade'
  | 'legacy-renamed-predecessor';

const LEGACY_RENAMED_RELATIONS = new Set([
  'agents', 'delivery_state', 'edit_log', 'harness_log', 'hook_receipts', 'locks', 'memories',
  'memory_refs', 'plan_docs', 'plan_members', 'plans', 'refinements', 'run_files', 'run_log',
  'sessions', 'signal_reads', 'signals', 'task_claims', 'task_dependencies', 'task_events',
  'task_paths', 'task_runs', 'tasks',
]);

export const LEGACY_RELATION_DESTINATIONS: Readonly<Record<string, string>> = Object.freeze({
  agents: 'awareness_agents',
  locks: 'awareness_locks',
  memories: 'awareness_memories',
  plans: 'awareness_plans',
  tasks: 'awareness_tasks',
});

export const LEGACY_DEFAULTED_COLUMNS: Readonly<Record<string, readonly string[]>> = Object.freeze({
  agents: ['role', 'status', 'metadata_json'],
  memories: ['scope_kind', 'source_digest', 'verified_at', 'secret_scan_status'],
  plans: ['source_kind', 'source_key', 'rfc_path', 'rfc_revision'],
  tasks: ['source_step_key', 'check_command'],
});

function isLegacyRenamedPredecessor(db: DatabaseSync, identity: SchemaIdentity): boolean {
  if (identity.applicationId !== AWARENESS_APPLICATION_ID) return false;
  const relations = identity.relations
    .filter(({ name }) => !/^memories_fts(?:_|$)/.test(name))
    .map(({ name }) => name);
  if (relations.length !== LEGACY_RENAMED_RELATIONS.size
    || !relations.every((name) => LEGACY_RENAMED_RELATIONS.has(name))) return false;
  const expected = new DatabaseSync(':memory:');
  try {
    expected.exec(LEGACY_RENAMED_V1_SCHEMA_DDL);
    if (identity.relations.some(({ name }) => name === 'memories_fts')) expected.exec(FTS_SCHEMA_DDL);
    try {
      assertSchemaObjects(readSchemaObjects(db), readSchemaObjects(expected));
    } catch (error) {
      throw new Error(`legacy-renamed-v1 schema fingerprint mismatch: ${(error as Error).message}`);
    }
    return true;
  } finally {
    expected.close();
  }
}

export function stableIdentityHash(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

export function legacyStoreIdForDatabasePath(databasePath: string): string {
  return stableIdentityHash(realpathSync(databasePath));
}

function readAwarenessMetaVersion(db: DatabaseSync, allowedVersions: readonly number[]): AwarenessMeta {
  const rows = db.prepare(`SELECT application_id, schema_version, store_id, created_at, last_migrated_at
    FROM awareness_meta`).all() as Array<{
      application_id: number;
      schema_version: number;
      store_id: string;
      created_at: string;
      last_migrated_at: string | null;
    }>;
  if (rows.length !== 1) throw new Error(`Awareness metadata requires exactly one row; found ${rows.length}`);
  const row = rows[0]!;
  if (row.application_id !== AWARENESS_APPLICATION_ID) {
    throw new Error(`Awareness metadata application_id ${row.application_id} does not match ${AWARENESS_APPLICATION_ID}`);
  }
  if (!allowedVersions.includes(row.schema_version)) {
    throw new Error(`unsupported Awareness schema_version ${row.schema_version}; expected ${allowedVersions.join(' or ')}`);
  }
  if (!/^(?:[0-9a-f]{64}|[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})$/.test(row.store_id)) {
    throw new Error('Awareness metadata contains an invalid store_id');
  }
  return {
    applicationId: row.application_id,
    schemaVersion: row.schema_version,
    storeId: row.store_id,
    createdAt: row.created_at,
    lastMigratedAt: row.last_migrated_at,
  };
}

export function readAwarenessMeta(db: DatabaseSync): AwarenessMeta {
  return readAwarenessMetaVersion(db, [AWARENESS_SCHEMA_VERSION]);
}

/** Read metadata from an exact copy-on-write predecessor without accepting arbitrary versions. */
export function readMigrationAwarenessMeta(db: DatabaseSync): AwarenessMeta | null {
  const hasMeta = db.prepare("SELECT 1 AS present FROM sqlite_schema WHERE type='table' AND name='awareness_meta'").get();
  return hasMeta
    ? readAwarenessMetaVersion(db, [2, ...AWARENESS_MIGRATABLE_SCHEMA_VERSIONS, AWARENESS_SCHEMA_VERSION])
    : null;
}

export function resolveAwarenessStoreIdentity(db: DatabaseSync): AwarenessMeta & { persisted: boolean } {
  const hasMeta = db.prepare("SELECT 1 AS present FROM sqlite_schema WHERE type='table' AND name='awareness_meta'").get();
  if (hasMeta) return { ...readAwarenessMeta(db), persisted: true };
  const row = db.prepare("PRAGMA database_list").all()
    .find((entry) => (entry as { name?: string }).name === 'main') as { file?: string } | undefined;
  if (!row?.file) throw new Error('an unversioned in-memory Awareness store has no stable identity');
  return {
    applicationId: AWARENESS_APPLICATION_ID,
    schemaVersion: 1,
    storeId: legacyStoreIdForDatabasePath(row.file),
    createdAt: '',
    lastMigratedAt: null,
    persisted: false,
  };
}

export function readSchemaIdentity(db: DatabaseSync): SchemaIdentity {
  const application = db.prepare('PRAGMA application_id').get() as { application_id: number };
  const relations = readSchemaObjects(db).filter(({ type }) => type === 'table' || type === 'view').map(({ name, type }) => ({ name, type }));
  return {
    applicationId: application.application_id ?? 0,
    relations,
  };
}

export function inspectSchemaState(db: DatabaseSync): SchemaState {
  const identity = readSchemaIdentity(db);
  const expected = new Set(canonicalColumns().keys());
  const relationNames = new Set(identity.relations.map(({ name }) => name));
  const canonicalCount = [...expected].filter((name) => relationNames.has(name)).length;
  const hasWorkerLifecycle = relationNames.has('worker_lifecycle_events');
  const hasRefinements = relationNames.has('refinements');
  const knownAwarenessHost = identity.relations.every(({ name, type }) => (
    type === 'table' && (expected.has(name) || name === 'memories_fts' || name === 'worker_lifecycle_events' || name === 'refinements')
  ));
  if (identity.applicationId === 0) {
    if (identity.relations.length === 0) return 'fresh';
    throw new Error('refusing unrecognized application_id=0 Awareness store; select a current canonical store or a fresh database. The database has not been changed.');
  }
  if (identity.applicationId === AWARENESS_APPLICATION_ID) {
    if (isLegacyRenamedPredecessor(db, identity)) return 'legacy-renamed-predecessor';
    if (isEventStreamConvergencePredecessor(db, identity)) return 'event-stream-convergence-upgrade';
    if (!knownAwarenessHost) {
      const names = identity.relations.map(({ name }) => name).join(', ');
      throw new Error(`refusing unrecognized or unrelated Awareness SQLite store; database consolidation may be required; relations: ${names}`);
    }
    const missingMeta = !relationNames.has('awareness_meta');
    const missingDurability = !relationNames.has('local_history_durability');
    const eventColumns = tableColumns(db, 'event_outbox');
    const currentEventEnvelope = eventColumns.has('schema_version') && eventColumns.has('retention_class');
    const predecessorEventEnvelope = !eventColumns.has('schema_version') && !eventColumns.has('retention_class');
    if (!currentEventEnvelope && !predecessorEventEnvelope) {
      throw new Error('Awareness requires the exact current event envelope; this database is not supported and has not been changed.');
    }
    const omittedTables = [
      ...(missingMeta ? ['awareness_meta'] : []),
      ...(missingDurability ? ['local_history_durability'] : []),
    ];
    if (predecessorEventEnvelope) {
      assertSchemaFingerprint(db, { omittedTables, eventOutboxVersion: 1, includeRefinements: hasRefinements });
      if (!missingMeta) readAwarenessMetaVersion(db, [2]);
      if (missingMeta && missingDurability) return 'event-envelope-history-durability-path-identity-upgrade';
      if (missingMeta) return 'event-envelope-path-identity-upgrade';
      if (missingDurability) return 'event-envelope-history-durability-upgrade';
      return 'event-envelope-upgrade';
    }
    if (canonicalCount !== expected.size) {
      if (canonicalCount === expected.size - 1 && missingMeta) {
        assertSchemaFingerprint(db, { omittedTables: ['awareness_meta'], includeRefinements: hasRefinements });
        return hasWorkerLifecycle ? 'worker-lifecycle-path-identity-upgrade' : 'canonical-path-identity';
      }
      if (canonicalCount === expected.size - 1 && missingDurability) {
        // Match the complete predecessor fingerprint before any migration write.
        assertSchemaFingerprint(db, { omittedTables: ['local_history_durability'], includeRefinements: hasRefinements });
        return hasWorkerLifecycle ? 'worker-lifecycle-history-durability-upgrade' : 'history-durability-upgrade';
      }
      if (canonicalCount === expected.size - 2 && missingMeta && missingDurability) {
        assertSchemaFingerprint(db, { omittedTables: ['awareness_meta', 'local_history_durability'], includeRefinements: hasRefinements });
        return hasWorkerLifecycle
          ? 'worker-lifecycle-history-durability-path-identity-upgrade'
          : 'history-durability-path-identity-upgrade';
      }
      throw new Error('Awareness requires the exact current canonical schema; this database is not supported and has not been changed. Select a fresh Awareness store.');
    }
    if (hasRefinements) {
      assertSchemaFingerprint(db, { includeRefinements: true });
      readAwarenessMeta(db);
      return hasWorkerLifecycle ? 'worker-lifecycle-upgrade' : 'refinements-upgrade';
    }
    assertCanonicalRelationContract(db, identity.relations);
    assertCanonicalSchemaFingerprint(db);
    const metadata = readAwarenessMetaVersion(
      db,
      [...AWARENESS_MIGRATABLE_SCHEMA_VERSIONS, AWARENESS_SCHEMA_VERSION],
    );
    if (metadata.schemaVersion !== AWARENESS_SCHEMA_VERSION && !hasWorkerLifecycle) {
      return 'schema-generation-upgrade';
    }
    return hasWorkerLifecycle ? 'worker-lifecycle-upgrade' : 'canonical';
  }
  if (identity.applicationId === AGENT_APPLICATION_ID) {
    throw new Error(`refusing Agent SQLite store; Awareness requires application_id ${AWARENESS_APPLICATION_ID}`);
  }
  throw new Error(
    `refusing foreign Awareness application_id ${identity.applicationId}; expected ${AWARENESS_APPLICATION_ID}`,
  );
}

export function assertDatabaseIntegrity(db: DatabaseSync): void {
  const integrity = db.prepare('PRAGMA integrity_check').all() as Array<{ integrity_check: string }>;
  const failures = integrity.filter(({ integrity_check }) => integrity_check !== 'ok');
  if (failures.length > 0) {
    throw new Error(`canonical integrity_check failed: ${failures.map((row) => row.integrity_check).join('; ')}`);
  }
  const foreignKeys = db.prepare('PRAGMA foreign_key_check').all();
  if (foreignKeys.length > 0) {
    throw new Error(`canonical foreign_key_check failed with ${foreignKeys.length} row(s)`);
  }
}
