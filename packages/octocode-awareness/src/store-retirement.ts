import { createHash, randomUUID } from 'node:crypto';
import { renameSync } from 'node:fs';
import { DatabaseSync } from './sqlite.js';
import { inspectSchemaState, readAwarenessMeta } from './db-introspection.js';
import { StoreRetirementError } from './store-retirement-error.js';
import { storeRetirementBlockers } from './store-retirement-state.js';
import {
  assertFreshRetirementTargets,
  assertRuntimeRetirementTargets,
  buildRetirementTargets,
  exactDatabasePath,
  exactWorkspacePath,
  pathEntryExists,
  recordedWorkspacePath,
  rollbackRetirementRenames,
  snapshotRetirementTarget,
  SQLITE_SUFFIXES,
} from './store-retirement-targets.js';
import type {
  ApplyStoreRetirementInput,
  StoreRetirementInput,
  StoreRetirementReport,
  StoreRetirementResult,
  StoreRetirementTarget,
} from './store-retirement-types.js';

export type {
  ApplyStoreRetirementInput,
  StoreRetirementBlocker,
  StoreRetirementBlockerCode,
  StoreRetirementInput,
  StoreRetirementReport,
  StoreRetirementResult,
  StoreRetirementTarget,
} from './store-retirement-types.js';

const MAX_RETIREMENT_WORKSPACES = 1_000;
const RETIREMENT_BUSY_TIMEOUT_MS = 100;
export { StoreRetirementError } from './store-retirement-error.js';

function quoteIdentifier(value: string): string {
  return `"${value.replaceAll('"', '""')}"`;
}

function openCanonicalDatabase(database: string, readOnly: boolean): DatabaseSync {
  const db = new DatabaseSync(database, { readOnly });
  try {
    db.exec(`PRAGMA busy_timeout = ${RETIREMENT_BUSY_TIMEOUT_MS}`);
    const state = inspectSchemaState(db);
    if (state !== 'canonical') {
      throw new StoreRetirementError('STORE_RETIREMENT_SCHEMA', `Store retirement requires the current canonical Awareness store; found ${state}.`);
    }
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

function recordedWorkspaces(db: DatabaseSync): string[] {
  const tables = db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
    .all() as Array<{ name: string }>;
  const values = new Set<string>();
  for (const { name } of tables) {
    const columns = db.prepare('SELECT name FROM pragma_table_info(?)').all(name) as Array<{ name: string }>;
    if (!columns.some(column => column.name === 'workspace_path')) continue;
    const rows = db.prepare(`SELECT DISTINCT workspace_path AS workspace
      FROM ${quoteIdentifier(name)}
      WHERE typeof(workspace_path)='text' AND trim(workspace_path)<>''
      LIMIT ${MAX_RETIREMENT_WORKSPACES + 1}`).all() as Array<{ workspace: string }>;
    for (const row of rows) {
      values.add(row.workspace);
      if (values.size > MAX_RETIREMENT_WORKSPACES) {
        throw new StoreRetirementError('STORE_RETIREMENT_WORKSPACE_LIMIT', `Store retirement found more than ${MAX_RETIREMENT_WORKSPACES} workspaces; no targets were changed.`);
      }
    }
  }
  return [...values].sort();
}

function planDigest(report: Omit<StoreRetirementReport, 'plan_digest'>): string {
  return createHash('sha256').update(JSON.stringify(report)).digest('hex');
}

function normalizedWorkspaceSet(db: DatabaseSync, requested: readonly string[]): string[] {
  const combined = new Set([
    ...recordedWorkspaces(db).map(recordedWorkspacePath),
    ...requested,
  ]);
  if (combined.size > MAX_RETIREMENT_WORKSPACES) {
    throw new StoreRetirementError('STORE_RETIREMENT_WORKSPACE_LIMIT', `Store retirement found more than ${MAX_RETIREMENT_WORKSPACES} workspaces; no targets were changed.`);
  }
  return [...combined].sort();
}

export function reportStoreRetirement(input: StoreRetirementInput): StoreRetirementReport {
  const database = exactDatabasePath(input.database);
  const requested = (input.workspaces ?? []).map(exactWorkspacePath);
  const db = openCanonicalDatabase(database, true);
  try {
    const identity = readAwarenessMeta(db);
    const workspaces = normalizedWorkspaceSet(db, requested);
    const blockers = storeRetirementBlockers(db, new Date().toISOString());
    const reportId = randomUUID();
    const targets = buildRetirementTargets(database, identity.storeId, workspaces, reportId);
    const unsigned: Omit<StoreRetirementReport, 'plan_digest'> = {
      action: 'report',
      dry_run: true,
      report_id: reportId,
      generated_at: new Date().toISOString(),
      confirmation: 'retire',
      writer_probe: 'deferred_to_apply',
      can_apply: blockers.length === 0,
      database: { path: database, store_id: identity.storeId },
      workspaces,
      blockers,
      targets,
    };
    return { ...unsigned, plan_digest: planDigest(unsigned) };
  } finally {
    db.close();
  }
}

function validateReport(report: StoreRetirementReport): void {
  const { plan_digest: digest, ...unsigned } = report;
  if (report.action !== 'report' || report.dry_run !== true || report.confirmation !== 'retire'
    || report.writer_probe !== 'deferred_to_apply' || report.can_apply !== (report.blockers.length === 0)
    || !/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(report.report_id)
    || digest !== planDigest(unsigned)) {
    throw new StoreRetirementError('STORE_RETIREMENT_REPORT_INVALID', 'Store retirement report is invalid or has been modified; run a new dry-run report.');
  }
}

export function applyStoreRetirement(input: ApplyStoreRetirementInput): StoreRetirementResult {
  if (input.confirm !== 'retire') {
    throw new StoreRetirementError('STORE_RETIREMENT_CONFIRMATION_REQUIRED', "Store retirement apply requires confirm: 'retire'.");
  }
  validateReport(input.report);
  if (input.report.blockers.length > 0 || !input.report.can_apply) {
    throw new StoreRetirementError('STORE_RETIREMENT_ACTIVE_STATE', 'Store retirement refuses active state; resolve every reported blocker and run a new report.');
  }
  const database = exactDatabasePath(input.report.database.path);
  const requestedWorkspaces = input.report.workspaces.map(recordedWorkspacePath).sort();
  const preflightTargets = buildRetirementTargets(database, input.report.database.store_id, requestedWorkspaces, input.report.report_id);
  assertFreshRetirementTargets(input.report, preflightTargets);
  const db = openCanonicalDatabase(database, false);
  let locked = false;
  const moved: StoreRetirementTarget[] = [];
  try {
    const identity = readAwarenessMeta(db);
    if (identity.storeId !== input.report.database.store_id) {
      throw new StoreRetirementError('STORE_RETIREMENT_STALE', 'Awareness store identity changed since report; run a new dry-run report.');
    }
    try {
      db.exec('BEGIN EXCLUSIVE');
      locked = true;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (/busy|locked/i.test(message)) {
        throw new StoreRetirementError('STORE_RETIREMENT_ACTIVE_WRITER', 'Store retirement refused an active SQLite writer; stop Awareness writers and retry from a new report.');
      }
      throw error;
    }
    const workspaces = normalizedWorkspaceSet(db, input.report.workspaces);
    if (JSON.stringify(workspaces) !== JSON.stringify(input.report.workspaces)) {
      throw new StoreRetirementError('STORE_RETIREMENT_STALE', 'Recorded workspace ownership changed since report; run a new dry-run report.');
    }
    const blockers = storeRetirementBlockers(db, new Date().toISOString());
    if (blockers.length > 0) {
      throw new StoreRetirementError('STORE_RETIREMENT_ACTIVE_STATE', 'Store retirement refuses active state; resolve every blocker and run a new report.');
    }
    const targets = buildRetirementTargets(database, identity.storeId, workspaces, input.report.report_id);
    assertRuntimeRetirementTargets(preflightTargets, targets, database);

    // Move LocalGit while the SQLite writer fence is held. Awareness history
    // writers claim SQLite state before publishing objects, so none can enter
    // the sidecar during this stage.
    for (const target of targets.filter(target => target.exists && target.kind !== 'sqlite')) {
      renameSync(target.source, target.quarantine);
      moved.push(target);
    }
    db.exec('ROLLBACK');
    locked = false;
    db.close();

    // The main file moves first after releasing SQLite's handle, preventing new
    // path-based opens from attaching to the retiring store. Every move remains
    // a recoverable sibling rename on the same filesystem.
    const settledTargets = [
      ...targets.filter(target => target.kind !== 'sqlite'),
      ...targets.filter(target => target.kind === 'sqlite')
        .map(target => snapshotRetirementTarget(target.kind, target.source, target.quarantine)),
    ];
    const sqliteTargets = settledTargets.filter(target => target.exists && target.kind === 'sqlite');
    const main = sqliteTargets.find(target => target.source === database);
    const ordered = main ? [main, ...sqliteTargets.filter(target => target !== main)] : sqliteTargets;
    for (const target of ordered) {
      renameSync(target.source, target.quarantine);
      moved.push(target);
    }
    const recreated = SQLITE_SUFFIXES.map(suffix => `${database}${suffix}`).filter(pathEntryExists);
    if (recreated.length > 0) {
      throw new StoreRetirementError('STORE_RETIREMENT_WRITER_RACE', `SQLite paths reappeared during retirement: ${recreated.join(', ')}`);
    }
    return {
      action: 'apply',
      status: 'quarantined',
      report_id: input.report.report_id,
      database: input.report.database,
      writer_fence: 'exclusive_transaction_acquired',
      quarantined: moved.map(({ kind, source, quarantine }) => ({ kind, source, quarantine })),
      absent: settledTargets.filter(target => !target.exists).map(({ kind, source, quarantine }) => ({ kind, source, quarantine })),
      recovery: 'rename each quarantine path back to its exact source path before restarting Awareness writers',
    };
  } catch (error) {
    if (locked) {
      try { db.exec('ROLLBACK'); } catch { /* preserve original failure */ }
    }
    try { db.close(); } catch { /* already closed after writer fence */ }
    const rollbackFailures = rollbackRetirementRenames(moved);
    if (rollbackFailures.length > 0) {
      throw new StoreRetirementError(
        'STORE_RETIREMENT_PARTIAL_QUARANTINE',
        `Store retirement failed and rollback was incomplete. Keep writers stopped; recover these quarantine paths manually: ${rollbackFailures.join(', ')}`,
      );
    }
    throw error;
  }
}
