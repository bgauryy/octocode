import { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';
import type { SQLInputValue } from 'node:sqlite';
import { AWARENESS_APPLICATION_ID } from './storage-scope.js';

const SQLITE_AUXILIARY = /^(?:sqlite_|memories_fts(?:_|$))/;
type SqlScalar = Exclude<SQLInputValue, undefined>;

export interface DatabaseConsolidationOptions {
  /** Validate a private copy, then discard it without publishing destination. */
  dryRun?: boolean;
}

export function tableNames(db: DatabaseSync): string[] {
  return (db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").all() as Array<{ name: string }>)
    .map(({ name }) => name);
}
function columns(db: DatabaseSync, table: string): string[] {
  return (db.prepare(`PRAGMA table_info(${JSON.stringify(table)})`).all() as Array<{ name: string }>).map(({ name }) => name);
}
export function scalar(value: unknown, table: string, column: string): SqlScalar {
  if (value === null || typeof value === 'string' || typeof value === 'number' || typeof value === 'bigint' || value instanceof Uint8Array) return value;
  throw new Error(`unsupported SQLite value in ${table}.${column}`);
}
export function text(value: unknown, table: string, column: string): string {
  if (typeof value !== 'string' || !value.trim()) throw new Error(`unsupported source row: ${table}.${column} is required`);
  return value;
}
export function nullableText(value: unknown, table: string, column: string): string | null {
  if (value === null) return null;
  if (typeof value !== 'string') throw new Error(`unsupported source row: ${table}.${column} must be text`);
  return value;
}
export function assertValidSource(source: DatabaseSync): void {
  const application = source.prepare('PRAGMA application_id').get() as { application_id?: unknown };
  if (application.application_id !== 0 && application.application_id !== AWARENESS_APPLICATION_ID) {
    throw new Error(`unsupported source application_id ${String(application.application_id)}`);
  }
  const integrity = source.prepare('PRAGMA integrity_check').get() as { integrity_check?: unknown };
  if (integrity.integrity_check !== 'ok') throw new Error(`source integrity check failed: ${String(integrity.integrity_check)}`);
  const foreignKeys = source.prepare('PRAGMA foreign_key_check').all();
  if (foreignKeys.length > 0) throw new Error(`source foreign key check failed with ${foreignKeys.length} row(s)`);
}
/** History objects live beside the source DB and are not portable through SQLite-only consolidation. */
export function assertNoHistoryRowsForConsolidation(source: DatabaseSync): void {
  const names = new Set(tableNames(source));
  for (const table of ['local_history_operations', 'local_history_versions', 'local_history_restores']) {
    if (!names.has(table)) continue;
    const row = source.prepare(`SELECT 1 AS present FROM ${table} LIMIT 1`).get() as { present?: number } | undefined;
    if (row?.present === 1) {
      throw new Error('history-aware consolidation required: the source has path-bound local history objects; no destination was created');
    }
  }
}
export function copyMappedTables(
  source: DatabaseSync,
  destination: DatabaseSync,
  sourceToDestination: Readonly<Record<string, string>> = {},
  options: { omittedSourceTables?: ReadonlySet<string> } = {},
): Record<string, number> {
  const sourceNames = new Set(tableNames(source));
  const sourceByDestination = new Map(Object.entries(sourceToDestination).map(([sourceName, destinationName]) => [destinationName, sourceName]));
  const result: Record<string, number> = {};
  for (const destinationTable of tableNames(destination)) {
    const sourceTable = sourceByDestination.get(destinationTable) ?? destinationTable;
    if (!sourceNames.has(sourceTable) || SQLITE_AUXILIARY.test(sourceTable) || options.omittedSourceTables?.has(sourceTable)) continue;
    const destinationColumns = columns(destination, destinationTable);
    const sourceInfo = source.prepare(`PRAGMA table_info(${JSON.stringify(sourceTable)})`).all() as Array<{
      name: string; pk: number;
    }>;
    const sourceColumns = sourceInfo.map(({ name }) => name);
    if (destinationColumns.length === 0) continue;
    const destinationInfo = destination.prepare(`PRAGMA table_info(${JSON.stringify(destinationTable)})`).all() as Array<{ name: string; notnull: number; dflt_value: string | null }>;
    const extra = sourceColumns.filter((column) => !destinationColumns.includes(column));
    if (extra.length > 0) throw new Error(`unsupported source schema: ${sourceTable} has unmappable columns ${extra.join(', ')}`);
    for (const column of destinationInfo) {
      if (!sourceColumns.includes(column.name) && column.notnull !== 0 && column.dflt_value === null) {
        throw new Error(`unsupported source schema: ${sourceTable} lacks required column ${column.name}`);
      }
    }
    const selectedColumns = destinationColumns.filter((column) => sourceColumns.includes(column));
    if (selectedColumns.length === 0) throw new Error(`unsupported source schema: ${sourceTable} has no mappable columns`);
    const quoted = selectedColumns.map((column) => JSON.stringify(column)).join(', ');
    const primaryKey = sourceInfo.filter(({ pk }) => pk > 0).sort((a, b) => a.pk - b.pk).map(({ name }) => JSON.stringify(name));
    const orderBy = primaryKey.length > 0 ? ` ORDER BY ${primaryKey.join(', ')}` : ' ORDER BY rowid';
    const rows = source.prepare(`SELECT ${quoted} FROM ${JSON.stringify(sourceTable)}${orderBy}`).all() as Array<Record<string, unknown>>;
    if (rows.length === 0) { result[sourceTable] = 0; continue; }
    const insert = destination.prepare(`INSERT INTO ${JSON.stringify(destinationTable)} (${quoted}) VALUES (${selectedColumns.map((column) => `@${column}`).join(', ')})`);
    for (const row of rows) {
      const values: Record<string, SQLInputValue> = {};
      for (const column of selectedColumns) {
        values[column] = scalar(row[column], sourceTable, column);
      }
      insert.run(values);
    }
    result[sourceTable] = rows.length;
  }
  return result;
}

export function copyCommonTables(source: DatabaseSync, destination: DatabaseSync): Record<string, number> {
  return copyMappedTables(source, destination);
}

/** Checks coordination invariants that SQLite foreign keys cannot express. */
export function assertLogicalDestination(destination: DatabaseSync): void {
  const failures: string[] = [];
  const crossPlan = destination.prepare(`SELECT d.task_id, d.depends_on_task_id
    FROM task_dependencies d
    JOIN awareness_tasks task ON task.task_id = d.task_id
    JOIN awareness_tasks dependency ON dependency.task_id = d.depends_on_task_id
    WHERE task.plan_id <> dependency.plan_id
    ORDER BY d.task_id LIMIT 1`).get() as { task_id: string; depends_on_task_id: string } | undefined;
  if (crossPlan) failures.push(`cross-plan dependency ${crossPlan.task_id}->${crossPlan.depends_on_task_id}`);
  const cycle = destination.prepare(`WITH RECURSIVE reach(start_task_id, task_id) AS (
      SELECT task_id, depends_on_task_id FROM task_dependencies
      UNION
      SELECT reach.start_task_id, dependency.depends_on_task_id
      FROM reach JOIN task_dependencies dependency ON dependency.task_id = reach.task_id
    ) SELECT start_task_id FROM reach WHERE start_task_id = task_id LIMIT 1`).get() as { start_task_id: string } | undefined;
  if (cycle) failures.push(`cyclic dependency at ${cycle.start_task_id}`);
  const claim = destination.prepare(`SELECT claim.task_id, claim.run_id
    FROM task_claims claim JOIN task_runs run ON run.run_id = claim.run_id
    WHERE run.task_id IS NOT claim.task_id OR run.agent_id <> claim.agent_id LIMIT 1`).get() as { task_id: string; run_id: string } | undefined;
  if (claim) failures.push(`claim/run mismatch ${claim.task_id}/${claim.run_id}`);
  const workspace = destination.prepare(`SELECT run.run_id
    FROM task_runs run
    JOIN awareness_tasks task ON task.task_id = run.task_id
    JOIN awareness_plans plan ON plan.plan_id = task.plan_id
    WHERE run.task_id IS NOT NULL AND run.workspace_path IS NOT plan.workspace_path LIMIT 1`).get() as { run_id: string } | undefined;
  if (workspace) failures.push(`run workspace mismatch ${workspace.run_id}`);
  if (failures.length > 0) throw new Error(`destination logical validation failed: ${failures.join('; ')}`);
}
