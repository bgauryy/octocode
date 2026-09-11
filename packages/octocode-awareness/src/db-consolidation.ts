import { createHash } from 'node:crypto';
import { chmodSync, existsSync, linkSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';
import { FTS_SCHEMA_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL } from './db-schema.js';
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
} from './db-introspection.js';
import { AWARENESS_APPLICATION_ID, AWARENESS_SCHEMA_VERSION } from './storage-scope.js';
import { WORKER_LIFECYCLE_DDL } from './db-worker-schema.js';
import {
  MIGRATED_EVENT_TABLES,
  NON_DIRECT_MIGRATION_TABLES,
  assertLogicalDestination,
  assertNoHistoryRowsForConsolidation,
  assertValidSource,
  copyCommonTables,
  copyMappedTables,
  copySequenceHighWaterMarks,
  copySyntheticEvents,
  eventReplayPlan,
  tableNames,
} from './db-consolidation-validation.js';
import { copyLegacyHandoffSignals } from './db-consolidation-handoffs.js';
import { assertClassifiableRefinements, copyClassifiedRefinements } from './db-consolidation-refinements.js';
import type { DatabaseConsolidationOptions } from './db-consolidation-validation.js';
import { verifyDatabaseMigration as verifyDatabaseMigrationImpl } from './db-migration-verification.js';
import {
  migrationSourceVersion,
  schemaStateForMigrationVersion,
  type DatabaseMigrationSourceVersion as DatabaseMigrationSourceVersionContract,
  type DatabaseMigrationVerification as DatabaseMigrationVerificationContract,
  type DatabaseMigrationVerificationRequest as DatabaseMigrationVerificationRequestContract,
} from './db-migration-contracts.js';
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

export const verifyDatabaseMigration = verifyDatabaseMigrationImpl;
export type DatabaseMigrationSourceVersion = DatabaseMigrationSourceVersionContract;
export type DatabaseMigrationVerification = DatabaseMigrationVerificationContract;
export type DatabaseMigrationVerificationRequest = DatabaseMigrationVerificationRequestContract;

function fileDigest(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
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
    assertClassifiableRefinements(source);
    const sourceMeta = readMigrationAwarenessMeta(source);
    const replayPlan = eventReplayPlan(source);
    destination = new DatabaseSync(temporaryPath);
    destination.exec('PRAGMA foreign_keys=OFF');
    destination.exec('BEGIN IMMEDIATE');
    destination.exec(SCHEMA_DDL);
    destination.exec(SCHEMA_INDEX_DDL);
    const copiedTables = copyMappedTables(source, destination, LEGACY_RELATION_DESTINATIONS, {
      omittedSourceTables: new Set(['awareness_meta', ...NON_DIRECT_MIGRATION_TABLES]),
    });
    if (sourceMeta) copiedTables.awareness_meta = 1;
    copySequenceHighWaterMarks(source, destination);
    if (tableNames(source).includes('refinements')) {
      copiedTables.refinements = copyClassifiedRefinements(source, destination);
    }
    Object.assign(copiedTables, copySyntheticEvents(source, destination));
    copyLegacyHandoffSignals(source, destination);
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
    assertClassifiableRefinements(sourceDb);
    const relations = tableNames(sourceDb).filter((name) => !SQLITE_OR_FTS_AUXILIARY.test(name));
    const copiedTables: Record<string, number> = {};
    const transformations = relations.map((relation) => {
      const count = sourceDb.prepare(`SELECT COUNT(*) AS count FROM ${JSON.stringify(relation)}`).get() as { count: number | bigint };
      const rows = Number(count.count);
      copiedTables[relation] = rows;
      const destination = relation === 'refinements'
        ? 'classified canonical owner'
        : MIGRATED_EVENT_TABLES.has(relation)
        ? 'event_outbox'
        : LEGACY_RELATION_DESTINATIONS[relation] ?? relation;
      const expectedColumns = canonicalColumns().get(destination) ?? [];
      const sourceColumns = tableColumns(sourceDb, relation);
      const defaultedColumns = NON_DIRECT_MIGRATION_TABLES.has(relation)
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
