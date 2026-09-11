import { createHash } from 'node:crypto';
import { chmodSync, existsSync, linkSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';
import { AWARENESS_SCHEMA_VERSION, FTS_SCHEMA_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL } from './db-schema.js';
import { hasFts, rebuildFts } from './db-maintenance.js';
import {
  assertCanonicalRelationContract,
  assertCanonicalSchemaFingerprint,
  assertDatabaseIntegrity,
  canonicalColumns,
  inspectSchemaState,
  LEGACY_DEFAULTED_COLUMNS,
  LEGACY_RELATION_DESTINATIONS,
  legacyStoreIdForDatabasePath,
  readAwarenessMeta,
  readMigrationAwarenessMeta,
  tableColumns,
  type SchemaState,
} from './db-introspection.js';
import { AWARENESS_APPLICATION_ID } from './storage-scope.js';
import { WORKER_LIFECYCLE_DDL } from './db-worker-schema.js';
import { assertLogicalDestination, assertNoHistoryRowsForConsolidation, assertValidSource, copyCommonTables, copyMappedTables, tableNames, text } from './db-consolidation-validation.js';
import type { DatabaseConsolidationOptions } from './db-consolidation-validation.js';
import { utcNow } from './helpers.js';
import { historyStoragePathsForIdentity } from './history-store.js';

export interface DatabaseConsolidationReport {
  dryRun: boolean;
  sourcePath: string;
  destinationPath: string;
  copiedTables: Readonly<Record<string, number>>;
  adoptedAgentIds: readonly string[];
}

const SQLITE_OR_FTS_AUXILIARY = /^(?:sqlite_|memories_fts(?:_|$))/;

export type DatabaseMigrationSourceVersion =
  | 'legacy-renamed-v1'
  | 'canonical-path-identity'
  | 'event-envelope-upgrade'
  | 'event-envelope-path-identity-upgrade'
  | 'event-envelope-history-durability-upgrade'
  | 'event-envelope-history-durability-path-identity-upgrade'
  | 'worker-lifecycle-upgrade'
  | 'worker-lifecycle-path-identity-upgrade'
  | 'worker-lifecycle-history-durability-upgrade'
  | 'worker-lifecycle-history-durability-path-identity-upgrade';

export interface DatabaseMigrationPreview {
  dryRun: true;
  sourceVersion: DatabaseMigrationSourceVersion;
  sourcePath: string;
  destinationPath: string;
  storeId: string;
  sourceUnchanged: true;
  copiedTables: Readonly<Record<string, number>>;
  transformations: ReadonlyArray<{
    source: string;
    destination: string;
    rows: number;
    defaultedColumns: readonly string[];
  }>;
  createdRelations: readonly string[];
  omissions: ReadonlyArray<{ source: string; reason: string }>;
  localGit: null | {
    storeId: string;
    sourceLayout: 'awareness-v1' | 'awareness-v2';
    destinationLayout: 'awareness-v1' | 'awareness-v2';
    sourceRoot: string;
    destinationRoot: string;
  };
}

export interface DatabaseMigrationVerificationRequest {
  sourcePath: string;
  destinationPath: string;
  sourceVersion: DatabaseMigrationSourceVersion;
  expectedStoreId: string;
  expectedCounts: Readonly<Record<string, number>>;
  expectedEventIds: readonly string[];
  expectedEventSequences: readonly number[];
  expectedEventHighWater: number;
  expectedLocalGitRoot?: string;
}

export interface DatabaseMigrationVerification {
  integrity: 'ok';
  foreignKeyViolations: 0;
  storeId: string;
  sourcePresent: boolean;
  destinationPresent: boolean;
  tableCountsVerified: number;
  eventOrderVerified: true;
  eventReplayVerified: true;
  localGitRootReachable: true | null;
  localGitObjectsVerified: number | null;
}

export interface DatabaseMigrationReport extends Omit<DatabaseMigrationPreview, 'dryRun' | 'localGit'> {
  dryRun: false;
  published: true;
  localGit: null | (NonNullable<DatabaseMigrationPreview['localGit']> & { retained: true });
  verification: DatabaseMigrationVerification;
  next: {
    verify: DatabaseMigrationVerificationRequest;
    cutover: {
      requiresApproval: true;
      databasePath: string;
      retainedSourcePath: string;
      instruction: string;
    };
  };
}

function fileDigest(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

const COPY_ON_WRITE_SOURCE_STATES = new Set<SchemaState>([
  'canonical-path-identity',
  'event-envelope-upgrade',
  'event-envelope-path-identity-upgrade',
  'event-envelope-history-durability-upgrade',
  'event-envelope-history-durability-path-identity-upgrade',
  'worker-lifecycle-upgrade',
  'worker-lifecycle-path-identity-upgrade',
  'worker-lifecycle-history-durability-upgrade',
  'worker-lifecycle-history-durability-path-identity-upgrade',
  'legacy-renamed-predecessor',
]);

function migrationSourceVersion(state: SchemaState): DatabaseMigrationSourceVersion {
  if (!COPY_ON_WRITE_SOURCE_STATES.has(state)) {
    throw new Error(`database migration does not support source state ${state}; source has not been changed`);
  }
  return state === 'legacy-renamed-predecessor' ? 'legacy-renamed-v1' : state as DatabaseMigrationSourceVersion;
}

function schemaStateForMigrationVersion(version: DatabaseMigrationSourceVersion): SchemaState {
  return version === 'legacy-renamed-v1' ? 'legacy-renamed-predecessor' : version;
}

function hasTable(db: DatabaseSync, table: string): boolean {
  return Boolean(db.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?").get(table));
}

function eventReplayPlan(source: DatabaseSync): { ids: string[]; sequences: number[]; highWater: number } {
  const events = hasTable(source, 'event_outbox')
    ? source.prepare('SELECT sequence,event_id FROM event_outbox ORDER BY sequence').all() as Array<{ sequence: number | bigint; event_id: string }>
    : [];
  const ids = events.map(({ event_id }) => event_id);
  const sequences = events.map(({ sequence }) => Number(sequence));
  let highWater = sequences.at(-1) ?? 0;
  if (hasTable(source, 'sqlite_sequence')) {
    const row = source.prepare("SELECT seq FROM sqlite_sequence WHERE name='event_outbox'").get() as { seq: number | bigint } | undefined;
    if (row) highWater = Math.max(highWater, Number(row.seq));
  }
  if (hasTable(source, 'worker_lifecycle_events')) {
    const workers = source.prepare('SELECT packet_id FROM worker_lifecycle_events ORDER BY sequence').all() as Array<{ packet_id: string }>;
    for (const { packet_id } of workers) {
      ids.push(`worker_lifecycle:${packet_id}`);
      sequences.push(++highWater);
    }
  }
  return { ids, sequences, highWater };
}

function copyWorkerLifecycleEvents(source: DatabaseSync, destination: DatabaseSync): number {
  if (!hasTable(source, 'worker_lifecycle_events')) return 0;
  const rows = source.prepare(`SELECT packet_id,workspace_path,session_id,worker_id,correlation_id,
      event_type,redaction,created_at,payload_json,recorded_at
    FROM worker_lifecycle_events ORDER BY sequence`).all() as Array<Record<string, unknown>>;
  const insert = destination.prepare(`INSERT INTO event_outbox
    (event_id,workspace_path,event_type,aggregate_kind,aggregate_id,aggregate_revision,actor_json,
      provenance_json,payload_json,session_id,correlation_id,created_at,expires_at,schema_version,retention_class)
    VALUES (?,?,?,?,?,?,?,?,?,?,?,?,NULL,1,'operational')`);
  for (const row of rows) {
    const packetId = text(row.packet_id, 'worker_lifecycle_events', 'packet_id');
    const workerId = text(row.worker_id, 'worker_lifecycle_events', 'worker_id');
    const eventType = text(row.event_type, 'worker_lifecycle_events', 'event_type');
    const redaction = text(row.redaction, 'worker_lifecycle_events', 'redaction');
    const recordedAt = text(row.recorded_at, 'worker_lifecycle_events', 'recorded_at');
    let payload: unknown;
    try { payload = JSON.parse(text(row.payload_json, 'worker_lifecycle_events', 'payload_json')); }
    catch { throw new Error(`unsupported source row: worker_lifecycle_events.payload_json is invalid JSON for ${packetId}`); }
    insert.run(
      `worker_lifecycle:${packetId}`,
      text(row.workspace_path, 'worker_lifecycle_events', 'workspace_path'),
      'worker.lifecycle',
      'worker',
      workerId,
      null,
      JSON.stringify({ kind: 'tool', id: workerId }),
      JSON.stringify({ source: 'harness', trust: 'attributed-data' }),
      JSON.stringify({ event_type: eventType, redaction, payload, recorded_at: recordedAt }),
      text(row.session_id, 'worker_lifecycle_events', 'session_id'),
      text(row.correlation_id, 'worker_lifecycle_events', 'correlation_id'),
      text(row.created_at, 'worker_lifecycle_events', 'created_at'),
    );
  }
  return rows.length;
}

function verifyLocalGitObjects(destination: DatabaseSync, root: string | undefined): { reachable: true | null; objects: number | null } {
  if (!root || !existsSync(root)) return { reachable: null, objects: null };
  const gitDir = join(root, 'repo.git');
  const oids = new Set<string>();
  if (hasTable(destination, 'local_history_operations')) {
    const rows = destination.prepare(`SELECT before_commit_oid,after_commit_oid FROM local_history_operations`).all() as Array<{
      before_commit_oid: string | null; after_commit_oid: string | null;
    }>;
    for (const row of rows) for (const oid of [row.before_commit_oid, row.after_commit_oid]) if (oid) oids.add(oid);
  }
  if (hasTable(destination, 'local_history_versions')) {
    const rows = destination.prepare('SELECT before_oid,after_oid FROM local_history_versions').all() as Array<{
      before_oid: string | null; after_oid: string | null;
    }>;
    for (const row of rows) for (const oid of [row.before_oid, row.after_oid]) if (oid) oids.add(oid);
  }
  for (const oid of oids) {
    if (!/^[0-9a-f]{40}$/.test(oid) || !existsSync(join(gitDir, 'objects', oid.slice(0, 2), oid.slice(2)))) {
      throw new Error(`migration LocalGit object is missing or invalid: ${oid}`);
    }
  }
  return { reachable: true, objects: oids.size };
}

export function verifyDatabaseMigration(request: DatabaseMigrationVerificationRequest): DatabaseMigrationVerification {
  if (!existsSync(request.sourcePath)) throw new Error(`migration source no longer exists: ${request.sourcePath}`);
  if (!existsSync(request.destinationPath)) throw new Error(`migration destination does not exist: ${request.destinationPath}`);
  const source = new DatabaseSync(request.sourcePath, { readOnly: true });
  const destination = new DatabaseSync(request.destinationPath, { readOnly: true });
  try {
    const expectedSourceState = schemaStateForMigrationVersion(request.sourceVersion);
    if (inspectSchemaState(source) !== expectedSourceState) throw new Error(`migration source no longer matches ${request.sourceVersion}`);
    if (inspectSchemaState(destination) !== 'canonical') throw new Error('migration destination is not canonical');
    const meta = readAwarenessMeta(destination);
    if (meta.storeId !== request.expectedStoreId) throw new Error('migration destination store_id does not match the source mapping');
    assertDatabaseIntegrity(destination);
    const workerCount = request.expectedCounts.worker_lifecycle_events ?? 0;
    for (const [sourceTable, expected] of Object.entries(request.expectedCounts)) {
      const sourceCount = source.prepare(`SELECT COUNT(*) AS count FROM ${JSON.stringify(sourceTable)}`).get() as { count: number | bigint };
      if (Number(sourceCount.count) !== expected) throw new Error(`migration source row-count changed for ${sourceTable}`);
      if (sourceTable === 'worker_lifecycle_events') {
        const packets = source.prepare('SELECT packet_id FROM worker_lifecycle_events ORDER BY sequence').all() as Array<{ packet_id: string }>;
        const mapped = destination.prepare('SELECT 1 AS present FROM event_outbox WHERE event_id=?');
        if (packets.some(({ packet_id }) => !mapped.get(`worker_lifecycle:${packet_id}`))) {
          throw new Error('migration row-count mismatch for worker_lifecycle_events->event_outbox');
        }
        continue;
      }
      const destinationTable = LEGACY_RELATION_DESTINATIONS[sourceTable] ?? sourceTable;
      const destinationCount = destination.prepare(`SELECT COUNT(*) AS count FROM ${JSON.stringify(destinationTable)}`).get() as { count: number | bigint };
      const destinationExpected = sourceTable === 'event_outbox' ? expected + workerCount : expected;
      if (Number(destinationCount.count) !== destinationExpected) throw new Error(`migration row-count mismatch for ${sourceTable}->${destinationTable}`);
    }
    const destinationEventOrder = destination.prepare('SELECT sequence,event_id FROM event_outbox ORDER BY sequence').all() as Array<{
      sequence: number | bigint; event_id: string;
    }>;
    if (JSON.stringify(destinationEventOrder.map(({ event_id }) => event_id)) !== JSON.stringify(request.expectedEventIds)
      || JSON.stringify(destinationEventOrder.map(({ sequence }) => Number(sequence))) !== JSON.stringify(request.expectedEventSequences)) {
      throw new Error('migration event replay order does not match the previewed source order');
    }
    const destinationHighWater = destination.prepare("SELECT seq FROM sqlite_sequence WHERE name='event_outbox'").get() as { seq: number | bigint } | undefined;
    if (Number(destinationHighWater?.seq ?? 0) !== request.expectedEventHighWater) {
      throw new Error('migration event sequence high-water mark does not match the previewed source');
    }
    const sourceEvents = hasTable(source, 'event_outbox') ? source.prepare(`SELECT event_id,workspace_path,event_type,aggregate_kind,aggregate_id,
      aggregate_revision,actor_json,provenance_json,payload_json,session_id,correlation_id,created_at,expires_at
      FROM event_outbox ORDER BY sequence`).all() as Array<Record<string, unknown>> : [];
    const destinationEvents = destination.prepare(`SELECT event_id,workspace_path,event_type,aggregate_kind,aggregate_id,
      aggregate_revision,actor_json,provenance_json,payload_json,session_id,correlation_id,created_at,expires_at,schema_version,retention_class
      FROM event_outbox ORDER BY sequence`).all() as Array<Record<string, unknown>>;
    for (let index = 0; index < sourceEvents.length; index += 1) {
      const expected = { ...sourceEvents[index], schema_version: 1, retention_class: 'delivery' };
      if (JSON.stringify(destinationEvents[index]) !== JSON.stringify(expected)) throw new Error(`migration event replay mismatch at source sequence ${index + 1}`);
    }
    if (hasTable(source, 'worker_lifecycle_events')) {
      const workers = source.prepare(`SELECT packet_id,workspace_path,session_id,worker_id,correlation_id,event_type,
        redaction,created_at,payload_json,recorded_at FROM worker_lifecycle_events ORDER BY sequence`).all() as Array<Record<string, unknown>>;
      for (let index = 0; index < workers.length; index += 1) {
        const worker = workers[index]!;
        const event = destinationEvents[sourceEvents.length + index]!;
        const expectedPayload = JSON.stringify({ event_type: worker.event_type, redaction: worker.redaction,
          payload: JSON.parse(String(worker.payload_json)), recorded_at: worker.recorded_at });
        if (event.event_id !== `worker_lifecycle:${worker.packet_id}` || event.workspace_path !== worker.workspace_path
          || event.event_type !== 'worker.lifecycle' || event.aggregate_kind !== 'worker' || event.aggregate_id !== worker.worker_id
          || event.aggregate_revision !== null || event.actor_json !== JSON.stringify({ kind: 'tool', id: worker.worker_id })
          || event.provenance_json !== JSON.stringify({ source: 'harness', trust: 'attributed-data' })
          || event.payload_json !== expectedPayload || event.session_id !== worker.session_id
          || event.correlation_id !== worker.correlation_id || event.created_at !== worker.created_at
          || event.expires_at !== null || event.schema_version !== 1 || event.retention_class !== 'operational') {
          throw new Error(`migration worker event replay mismatch at source sequence ${index + 1}`);
        }
      }
    }
    const localGit = verifyLocalGitObjects(destination, request.expectedLocalGitRoot);
    return {
      integrity: 'ok',
      foreignKeyViolations: 0,
      storeId: meta.storeId,
      sourcePresent: true,
      destinationPresent: true,
      tableCountsVerified: Object.keys(request.expectedCounts).length,
      eventOrderVerified: true,
      eventReplayVerified: true,
      localGitRootReachable: localGit.reachable,
      localGitObjectsVerified: localGit.objects,
    };
  } finally { source.close(); destination.close(); }
}

/** Apply the exact previewed predecessor conversion to a new atomically published file. */
export function applyDatabaseMigration(
  sourcePath: string,
  destinationPath: string,
  options: { workspace?: string } = {},
): DatabaseMigrationReport {
  const preview = previewDatabaseMigration(sourcePath, destinationPath, options);
  const beforeDigest = fileDigest(preview.sourcePath);
  const temporaryDirectory = mkdtempSync(join(dirname(preview.destinationPath), '.awareness-migration-'));
  chmodSync(temporaryDirectory, 0o700);
  const temporaryPath = join(temporaryDirectory, basename(preview.destinationPath));
  let source: DatabaseSync | undefined;
  let destination: DatabaseSync | undefined;
  try {
    source = new DatabaseSync(preview.sourcePath, { readOnly: true });
    source.exec('BEGIN');
    if (inspectSchemaState(source) !== schemaStateForMigrationVersion(preview.sourceVersion)) {
      throw new Error(`unsupported migration source; expected exact ${preview.sourceVersion} predecessor`);
    }
    assertValidSource(source);
    const sourceMeta = readMigrationAwarenessMeta(source);
    const replayPlan = eventReplayPlan(source);
    destination = new DatabaseSync(temporaryPath);
    destination.exec('PRAGMA foreign_keys=OFF');
    destination.exec('BEGIN IMMEDIATE');
    destination.exec(SCHEMA_DDL);
    destination.exec(SCHEMA_INDEX_DDL);
    const copiedTables = copyMappedTables(source, destination, LEGACY_RELATION_DESTINATIONS, {
      omittedSourceTables: new Set(['awareness_meta', 'worker_lifecycle_events']),
    });
    if (sourceMeta) copiedTables.awareness_meta = 1;
    copySequenceHighWaterMarks(source, destination);
    const workerRows = copyWorkerLifecycleEvents(source, destination);
    if (hasTable(source, 'worker_lifecycle_events')) copiedTables.worker_lifecycle_events = workerRows;
    const migratedAt = utcNow();
    destination.prepare(`INSERT INTO awareness_meta
        (application_id, schema_version, store_id, created_at, last_migrated_at)
      VALUES (?, ?, ?, ?, ?)`)
      .run(AWARENESS_APPLICATION_ID, AWARENESS_SCHEMA_VERSION, preview.storeId, sourceMeta?.createdAt || migratedAt, migratedAt);
    try { destination.exec(FTS_SCHEMA_DDL); } catch { /* FTS5 is optional in the embedded SQLite build. */ }
    if (hasFts(destination)) rebuildFts(destination);
    destination.exec(`PRAGMA application_id=${AWARENESS_APPLICATION_ID}`);
    assertLogicalDestination(destination);
    assertCanonicalRelationContract(destination);
    assertCanonicalSchemaFingerprint(destination);
    assertDatabaseIntegrity(destination);
    readAwarenessMeta(destination);
    destination.exec('COMMIT');
    destination.exec('PRAGMA foreign_keys=ON');
    source.exec('COMMIT');
    destination.exec('PRAGMA wal_checkpoint(TRUNCATE)');
    destination.close();
    destination = undefined;
    source.close();
    source = undefined;
    if (fileDigest(preview.sourcePath) !== beforeDigest) {
      throw new Error('migration source changed while copying; no destination was published');
    }
    const expectedStoreId = preview.storeId;
    const verification = verifyDatabaseMigration({
      sourcePath: preview.sourcePath,
      destinationPath: temporaryPath,
      sourceVersion: preview.sourceVersion,
      expectedStoreId,
      expectedCounts: copiedTables,
      expectedEventIds: replayPlan.ids,
      expectedEventSequences: replayPlan.sequences,
      expectedEventHighWater: replayPlan.highWater,
      expectedLocalGitRoot: preview.localGit?.destinationRoot,
    });
    chmodSync(temporaryPath, 0o600);
    try { linkSync(temporaryPath, preview.destinationPath); }
    catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'EEXIST') throw new Error(`destination already exists: ${preview.destinationPath}`);
      throw error;
    }
    const verify = {
      sourcePath: preview.sourcePath,
      destinationPath: preview.destinationPath,
      sourceVersion: preview.sourceVersion,
      expectedStoreId,
      expectedCounts: copiedTables,
      expectedEventIds: replayPlan.ids,
      expectedEventSequences: replayPlan.sequences,
      expectedEventHighWater: replayPlan.highWater,
      expectedLocalGitRoot: preview.localGit?.destinationRoot,
    };
    const transformations = preview.transformations.map((entry) => ({ ...entry, rows: copiedTables[entry.source] ?? 0 }));
    return {
      ...preview,
      dryRun: false,
      published: true,
      sourceUnchanged: true,
      copiedTables,
      transformations,
      localGit: preview.localGit ? { ...preview.localGit, retained: true } : null,
      verification,
      next: {
        verify,
        cutover: {
          requiresApproval: true,
          databasePath: preview.destinationPath,
          retainedSourcePath: preview.sourcePath,
          instruction: 'After independent verification, explicitly select the destination database in workspace policy; retain the source for rollback.',
        },
      },
    };
  } catch (error) {
    try { destination?.exec('ROLLBACK'); } catch { /* no active destination transaction */ }
    try { source?.exec('ROLLBACK'); } catch { /* read snapshot ended */ }
    try { destination?.close(); } catch { /* cleanup only */ }
    try { source?.close(); } catch { /* cleanup only */ }
    throw error;
  } finally {
    try { destination?.close(); } catch { /* cleanup only */ }
    try { source?.close(); } catch { /* cleanup only */ }
    rmSync(temporaryDirectory, { recursive: true, force: true });
  }
}

/**
 * Inspect a recognized predecessor without creating a destination or changing
 * source bytes. Copy-on-write application is intentionally a separate phase.
 */
export function previewDatabaseMigration(
  sourcePath: string,
  destinationPath: string,
  options: { workspace?: string } = {},
): DatabaseMigrationPreview {
  if (!existsSync(sourcePath)) throw new Error(`source database does not exist: ${sourcePath}`);
  const sourceResolved = realpathSync(sourcePath);
  const destinationResolved = resolve(destinationPath);
  if (sourceResolved === destinationResolved) throw new Error('destination must differ from source');
  if (existsSync(destinationResolved)) throw new Error(`destination already exists: ${destinationResolved}`);
  const sourceDb = new DatabaseSync(sourceResolved, { readOnly: true });
  try {
    const state = inspectSchemaState(sourceDb);
    const sourceVersion = migrationSourceVersion(state);
    assertValidSource(sourceDb);
    const relations = tableNames(sourceDb).filter((name) => !SQLITE_OR_FTS_AUXILIARY.test(name));
    const copiedTables: Record<string, number> = {};
    const transformations = relations.map((relation) => {
      const count = sourceDb.prepare(`SELECT COUNT(*) AS count FROM ${JSON.stringify(relation)}`).get() as { count: number | bigint };
      const rows = Number(count.count);
      copiedTables[relation] = rows;
      const destination = relation === 'worker_lifecycle_events'
        ? 'event_outbox'
        : LEGACY_RELATION_DESTINATIONS[relation] ?? relation;
      const expectedColumns = canonicalColumns().get(destination) ?? [];
      const sourceColumns = tableColumns(sourceDb, relation);
      const defaultedColumns = relation === 'worker_lifecycle_events'
        ? []
        : state === 'legacy-renamed-predecessor'
          ? LEGACY_DEFAULTED_COLUMNS[relation] ?? []
          : expectedColumns.filter(({ name, dflt_value }) => !sourceColumns.has(name) && dflt_value !== null).map(({ name }) => name);
      return {
        source: relation,
        destination,
        rows,
        defaultedColumns,
      };
    });
    const destinations = new Set(transformations.map(({ destination }) => destination));
    const workspace = options.workspace ? realpathSync(options.workspace) : null;
    const sourceMeta = readMigrationAwarenessMeta(sourceDb);
    const storeId = sourceMeta?.storeId ?? legacyStoreIdForDatabasePath(sourceResolved);
    const sourceStorage = workspace ? historyStoragePathsForIdentity(
      { workspace, dbPath: sourceResolved },
      { storeId, persisted: sourceMeta !== null },
    ) : null;
    const destinationStorage = workspace ? historyStoragePathsForIdentity(
      { workspace, dbPath: destinationResolved },
      { storeId, persisted: true },
    ) : null;
    return {
      dryRun: true,
      sourceVersion,
      sourcePath: sourceResolved,
      destinationPath: destinationResolved,
      storeId,
      sourceUnchanged: true,
      copiedTables,
      transformations,
      createdRelations: [...canonicalColumns().keys()].filter((relation) => !destinations.has(relation)).sort(),
      omissions: [],
      localGit: sourceStorage && destinationStorage ? {
        storeId,
        sourceLayout: sourceStorage.layout,
        destinationLayout: destinationStorage.layout,
        sourceRoot: sourceStorage.root,
        destinationRoot: destinationStorage.root,
      } : null,
    };
  } finally {
    sourceDb.close();
  }
}

function copySequenceHighWaterMarks(source: DatabaseSync, destination: DatabaseSync): void {
  if (!source.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name='sqlite_sequence'").get()) return;
  const rows = source.prepare('SELECT name, seq FROM sqlite_sequence').all() as Array<{ name: string; seq: number | bigint }>;
  for (const row of rows) {
    if (row.name !== 'event_outbox' && row.name !== 'worker_lifecycle_events') continue;
    if (!destination.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?").get(row.name)) continue;
    const existing = destination.prepare('SELECT seq FROM sqlite_sequence WHERE name=?').get(row.name) as { seq: number | bigint } | undefined;
    if (!existing) destination.prepare('INSERT INTO sqlite_sequence(name,seq) VALUES (?,?)').run(row.name, row.seq);
    else if (row.seq > existing.seq) destination.prepare('UPDATE sqlite_sequence SET seq=? WHERE name=?').run(row.seq, row.name);
  }
}

export function consolidateDatabase(sourcePath: string, destinationPath: string, options: DatabaseConsolidationOptions = {}): DatabaseConsolidationReport {
  if (sourcePath === destinationPath) throw new Error('destination must differ from source');
  if (!existsSync(sourcePath)) throw new Error(`source database does not exist: ${sourcePath}`);
  if (existsSync(destinationPath)) throw new Error(`destination already exists: ${destinationPath}`);
  const temporaryDirectory = mkdtempSync(join(dirname(destinationPath), '.awareness-consolidation-'));
  chmodSync(temporaryDirectory, 0o700);
  const temporaryPath = join(temporaryDirectory, basename(destinationPath));
  let source: DatabaseSync | undefined;
  let destination: DatabaseSync | undefined;
  try {
    source = new DatabaseSync(sourcePath, { readOnly: true });
    source.exec('BEGIN');
    assertValidSource(source);
    assertCanonicalRelationContract(source);
    assertCanonicalSchemaFingerprint(source);
    readAwarenessMeta(source);
    assertNoHistoryRowsForConsolidation(source);
    destination = new DatabaseSync(temporaryPath);
    destination.exec('PRAGMA foreign_keys=OFF');
    destination.exec('BEGIN IMMEDIATE');
    destination.exec(SCHEMA_DDL);
    destination.exec(SCHEMA_INDEX_DDL);
    if (new Set(tableNames(source)).has('worker_lifecycle_events')) destination.exec(WORKER_LIFECYCLE_DDL);
    const copiedTables = copyCommonTables(source, destination);
    copySequenceHighWaterMarks(source, destination);
    try { destination.exec(FTS_SCHEMA_DDL); } catch { /* FTS5 is optional in the embedded SQLite build. */ }
    if (hasFts(destination)) rebuildFts(destination);
    destination.exec(`PRAGMA application_id=${AWARENESS_APPLICATION_ID}`);
    assertLogicalDestination(destination);
    assertCanonicalRelationContract(destination);
    assertCanonicalSchemaFingerprint(destination);
    readAwarenessMeta(destination);
    const integrity = destination.prepare('PRAGMA integrity_check').get() as { integrity_check: string };
    if (integrity.integrity_check !== 'ok') throw new Error(`destination integrity check failed: ${integrity.integrity_check}`);
    if (destination.prepare('PRAGMA foreign_key_check').all().length > 0) throw new Error('destination foreign key check failed');
    destination.exec('COMMIT');
    destination.exec('PRAGMA foreign_keys=ON');
    source.exec('COMMIT');
    destination.exec('PRAGMA wal_checkpoint(TRUNCATE)');
    destination.close();
    destination = undefined;
    const report = { sourcePath, destinationPath, copiedTables, adoptedAgentIds: [], dryRun: options.dryRun === true };
    if (report.dryRun) return report;
    try { linkSync(temporaryPath, destinationPath); }
    catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'EEXIST') throw new Error(`destination already exists: ${destinationPath}`);
      throw error;
    }
    return report;
  } catch (error) {
    try { destination?.exec('ROLLBACK'); } catch { /* no active destination transaction */ }
    try { source?.exec('ROLLBACK'); } catch { /* read snapshot ended */ }
    try { destination?.close(); } catch { /* cleanup only */ }
    throw error;
  } finally {
    try { destination?.close(); } catch { /* cleanup only */ }
    source?.close();
    rmSync(temporaryDirectory, { recursive: true, force: true });
  }
}
