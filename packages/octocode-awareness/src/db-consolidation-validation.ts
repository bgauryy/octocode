import { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';
import type { SQLInputValue } from 'node:sqlite';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { LEGACY_RELATION_DESTINATIONS } from './db-introspection.js';
import { AWARENESS_APPLICATION_ID } from './storage-scope.js';
import { legacyHandoffEvents, type MigrationEvent } from './db-consolidation-handoffs.js';
import { refinementDestinationCounts, refinementMigrationEvents } from './db-consolidation-refinements.js';
import { signalExpiresAt } from './message-lifecycle.js';

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
    const derivesSignalExpiry = destinationTable === 'signals' && !sourceColumns.includes('expires_at');
    const extra = sourceColumns.filter((column) => !destinationColumns.includes(column));
    if (extra.length > 0) throw new Error(`unsupported source schema: ${sourceTable} has unmappable columns ${extra.join(', ')}`);
    for (const column of destinationInfo) {
      if (!sourceColumns.includes(column.name) && column.notnull !== 0 && column.dflt_value === null
        && !(derivesSignalExpiry && column.name === 'expires_at')) {
        throw new Error(`unsupported source schema: ${sourceTable} lacks required column ${column.name}`);
      }
    }
    const selectedColumns = destinationColumns.filter((column) => sourceColumns.includes(column));
    if (derivesSignalExpiry) selectedColumns.push('expires_at');
    if (selectedColumns.length === 0) throw new Error(`unsupported source schema: ${sourceTable} has no mappable columns`);
    const sourceSelectedColumns = selectedColumns.filter((column) => sourceColumns.includes(column));
    const quoted = sourceSelectedColumns.map((column) => JSON.stringify(column)).join(', ');
    const primaryKey = sourceInfo.filter(({ pk }) => pk > 0).sort((a, b) => a.pk - b.pk).map(({ name }) => JSON.stringify(name));
    const orderBy = primaryKey.length > 0 ? ` ORDER BY ${primaryKey.join(', ')}` : ' ORDER BY rowid';
    const rows = source.prepare(`SELECT ${quoted} FROM ${JSON.stringify(sourceTable)}${orderBy}`).all() as Array<Record<string, unknown>>;
    if (rows.length === 0) { result[sourceTable] = 0; continue; }
    const insertColumns = selectedColumns.map((column) => JSON.stringify(column)).join(', ');
    const insert = destination.prepare(`INSERT INTO ${JSON.stringify(destinationTable)} (${insertColumns}) VALUES (${selectedColumns.map((column) => `@${column}`).join(', ')})`);
    for (const row of rows) {
      const values: Record<string, SQLInputValue> = {};
      for (const column of selectedColumns) {
        values[column] = column === 'expires_at' && derivesSignalExpiry
          ? signalExpiresAt(String(row.kind), String(row.created_at))
          : scalar(row[column], sourceTable, column);
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

export function copySequenceHighWaterMarks(source: DatabaseSync, destination: DatabaseSync): void {
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

export const MIGRATED_EVENT_TABLES = new Set([
  'task_events', 'run_log', 'edit_log', 'harness_log', 'handoffs', 'worker_lifecycle_events',
]);
export const NON_DIRECT_MIGRATION_TABLES = new Set([...MIGRATED_EVENT_TABLES, 'refinements']);

export interface MigrationEventReplayPlan {
  ids: string[];
  sequences: number[];
  highWater: number;
}

export function migrationHasTable(db: DatabaseSync, table: string): boolean {
  return Boolean(db.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?").get(table));
}

function workspaceForLegacyEvent(source: DatabaseSync, row: Record<string, unknown>, table: string): string {
  if (typeof row.workspace_path === 'string' && row.workspace_path) return row.workspace_path;
  if (typeof row.task_id === 'string' && row.task_id) {
    const taskTable = migrationHasTable(source, 'awareness_tasks') ? 'awareness_tasks' : 'tasks';
    const planTable = migrationHasTable(source, 'awareness_plans') ? 'awareness_plans' : 'plans';
    const resolved = source.prepare(`SELECT plan.workspace_path FROM ${taskTable} task
      JOIN ${planTable} plan ON plan.plan_id=task.plan_id WHERE task.task_id=?`).get(row.task_id) as { workspace_path?: unknown } | undefined;
    if (typeof resolved?.workspace_path === 'string' && resolved.workspace_path) return resolved.workspace_path;
  }
  if (typeof row.run_id === 'string' && row.run_id) {
    const resolved = source.prepare('SELECT workspace_path FROM task_runs WHERE run_id=?').get(row.run_id) as { workspace_path?: unknown } | undefined;
    if (typeof resolved?.workspace_path === 'string' && resolved.workspace_path) return resolved.workspace_path;
  }
  if (typeof row.session_id === 'string' && row.session_id) {
    const resolved = source.prepare('SELECT workspace_path FROM sessions WHERE session_id=?').get(row.session_id) as { workspace_path?: unknown } | undefined;
    if (typeof resolved?.workspace_path === 'string' && resolved.workspace_path) return resolved.workspace_path;
  }
  if (typeof row.agent_id === 'string' && row.agent_id) {
    const agentTable = migrationHasTable(source, 'awareness_agents') ? 'awareness_agents' : 'agents';
    const candidates = source.prepare(`SELECT DISTINCT workspace_path FROM ${agentTable} WHERE agent_id=? ORDER BY workspace_path`)
      .all(row.agent_id) as Array<{ workspace_path: string }>;
    if (candidates.length === 1) return candidates[0]!.workspace_path;
  }
  throw new Error(`unsupported source row: ${table} cannot resolve one workspace_path`);
}

function actor(agentId: unknown, table: string): string {
  return JSON.stringify({ kind: 'agent', id: text(agentId, table, 'agent_id') });
}

function legacyEvents(source: DatabaseSync): MigrationEvent[] {
  const events: MigrationEvent[] = [];
  const provenance = JSON.stringify({ source: 'awareness', trust: 'attributed-data' });
  const append = (table: string, idColumn: string, prefix: string, retention: 'delivery' | 'operational',
    makeType: (row: Record<string, unknown>) => string,
    aggregate: (row: Record<string, unknown>) => { kind: string | null; id: string | null }) => {
    if (!migrationHasTable(source, table)) return;
    const rows = source.prepare(`SELECT * FROM ${JSON.stringify(table)} ORDER BY created_at, ${JSON.stringify(idColumn)}`).all() as Array<Record<string, unknown>>;
    for (const row of rows) {
      const id = text(row[idColumn], table, idColumn);
      const target = aggregate(row);
      events.push({
        event_id: `legacy.${prefix}:${id}`,
        workspace_path: workspaceForLegacyEvent(source, row, table),
        event_type: makeType(row),
        aggregate_kind: target.kind,
        aggregate_id: target.id,
        aggregate_revision: null,
        actor_json: actor(row.agent_id, table),
        provenance_json: provenance,
        payload_json: JSON.stringify(row),
        session_id: typeof row.session_id === 'string' ? row.session_id : null,
        correlation_id: typeof row.run_id === 'string' ? row.run_id : null,
        created_at: text(row.created_at, table, 'created_at'),
        expires_at: null,
        schema_version: 1,
        retention_class: retention,
      });
    }
  };
  append('task_events', 'event_id', 'task', 'delivery', row => `task.${text(row.event_type, 'task_events', 'event_type').toLowerCase()}`,
    row => ({ kind: 'task', id: text(row.task_id, 'task_events', 'task_id') }));
  append('run_log', 'event_id', 'run', 'delivery', row => `run.${text(row.event_type, 'run_log', 'event_type').toLowerCase()}`,
    row => ({ kind: typeof row.run_id === 'string' ? 'run' : null, id: typeof row.run_id === 'string' ? row.run_id : null }));
  append('edit_log', 'edit_id', 'edit', 'operational', row => `edit.${text(row.operation, 'edit_log', 'operation')}`,
    row => ({ kind: 'file', id: text(row.file_path, 'edit_log', 'file_path') }));
  append('harness_log', 'harness_id', 'harness', 'operational', row => `harness.${text(row.event_type, 'harness_log', 'event_type')}`,
    row => ({ kind: typeof row.memory_id === 'string' ? 'memory' : typeof row.run_id === 'string' ? 'run' : 'agent',
      id: typeof row.memory_id === 'string' ? row.memory_id : typeof row.run_id === 'string' ? row.run_id : text(row.agent_id, 'harness_log', 'agent_id') }));
  return [...events, ...legacyHandoffEvents(source)];
}

function workerEvents(source: DatabaseSync): MigrationEvent[] {
  if (!migrationHasTable(source, 'worker_lifecycle_events')) return [];
  const rows = source.prepare(`SELECT packet_id,workspace_path,session_id,worker_id,correlation_id,
      event_type,redaction,created_at,payload_json,recorded_at FROM worker_lifecycle_events ORDER BY sequence`).all() as Array<Record<string, unknown>>;
  return rows.map(row => {
    const packetId = text(row.packet_id, 'worker_lifecycle_events', 'packet_id');
    const workerId = text(row.worker_id, 'worker_lifecycle_events', 'worker_id');
    let payload: unknown;
    try { payload = JSON.parse(text(row.payload_json, 'worker_lifecycle_events', 'payload_json')); }
    catch { throw new Error(`unsupported source row: worker_lifecycle_events.payload_json is invalid JSON for ${packetId}`); }
    return {
      event_id: `worker_lifecycle:${packetId}`,
      workspace_path: text(row.workspace_path, 'worker_lifecycle_events', 'workspace_path'),
      event_type: 'worker.lifecycle', aggregate_kind: 'worker', aggregate_id: workerId, aggregate_revision: null,
      actor_json: JSON.stringify({ kind: 'tool', id: workerId }),
      provenance_json: JSON.stringify({ source: 'harness', trust: 'attributed-data' }),
      payload_json: JSON.stringify({ event_type: text(row.event_type, 'worker_lifecycle_events', 'event_type'),
        redaction: text(row.redaction, 'worker_lifecycle_events', 'redaction'), payload,
        recorded_at: text(row.recorded_at, 'worker_lifecycle_events', 'recorded_at') }),
      session_id: text(row.session_id, 'worker_lifecycle_events', 'session_id'),
      correlation_id: text(row.correlation_id, 'worker_lifecycle_events', 'correlation_id'),
      created_at: text(row.created_at, 'worker_lifecycle_events', 'created_at'), expires_at: null,
      schema_version: 1, retention_class: 'operational',
    };
  });
}

function syntheticEvents(source: DatabaseSync): MigrationEvent[] {
  return [...legacyEvents(source), ...workerEvents(source), ...refinementMigrationEvents(source)];
}

export function eventReplayPlan(source: DatabaseSync): MigrationEventReplayPlan {
  const rows = migrationHasTable(source, 'event_outbox')
    ? source.prepare('SELECT sequence,event_id FROM event_outbox ORDER BY sequence').all() as Array<{ sequence: number | bigint; event_id: string }>
    : [];
  const ids = rows.map(({ event_id }) => event_id);
  const sequences = rows.map(({ sequence }) => Number(sequence));
  let highWater = sequences.at(-1) ?? 0;
  if (migrationHasTable(source, 'sqlite_sequence')) {
    const row = source.prepare("SELECT seq FROM sqlite_sequence WHERE name='event_outbox'").get() as { seq: number | bigint } | undefined;
    if (row) highWater = Math.max(highWater, Number(row.seq));
  }
  for (const event of syntheticEvents(source)) { ids.push(event.event_id); sequences.push(++highWater); }
  return { ids, sequences, highWater };
}

export function copySyntheticEvents(source: DatabaseSync, destination: DatabaseSync): Record<string, number> {
  const insert = destination.prepare(`INSERT INTO event_outbox
    (event_id,workspace_path,event_type,aggregate_kind,aggregate_id,aggregate_revision,actor_json,
      provenance_json,payload_json,session_id,correlation_id,created_at,expires_at,schema_version,retention_class)
    VALUES (@event_id,@workspace_path,@event_type,@aggregate_kind,@aggregate_id,@aggregate_revision,@actor_json,
      @provenance_json,@payload_json,@session_id,@correlation_id,@created_at,@expires_at,@schema_version,@retention_class)`);
  for (const event of syntheticEvents(source)) {
    const values: Record<string, SQLInputValue> = { ...event };
    insert.run(values);
  }
  const counts: Record<string, number> = {};
  for (const table of MIGRATED_EVENT_TABLES) if (migrationHasTable(source, table)) {
    const row = source.prepare(`SELECT COUNT(*) AS count FROM ${JSON.stringify(table)}`).get() as { count: number | bigint };
    counts[table] = Number(row.count);
  }
  return counts;
}


export function verifyMigrationContent(source: DatabaseSync, destination: DatabaseSync, options: {
  expectedCounts: Readonly<Record<string, number>>;
  expectedEventIds: readonly string[];
  expectedEventSequences: readonly number[];
  expectedEventHighWater: number;
}): void {
  const synthetic = syntheticEvents(source);
  const refinementAdditions = refinementDestinationCounts(source);
  for (const [sourceTable, expected] of Object.entries(options.expectedCounts)) {
    const sourceCount = source.prepare(`SELECT COUNT(*) AS count FROM ${JSON.stringify(sourceTable)}`).get() as { count: number | bigint };
    if (Number(sourceCount.count) !== expected) throw new Error(`migration source row-count changed for ${sourceTable}`);
    if (NON_DIRECT_MIGRATION_TABLES.has(sourceTable)) continue;
    const destinationTable = LEGACY_RELATION_DESTINATIONS[sourceTable] ?? sourceTable;
    const destinationCount = destination.prepare(`SELECT COUNT(*) AS count FROM ${JSON.stringify(destinationTable)}`).get() as { count: number | bigint };
    const handoffCount = sourceTable === 'signals' && migrationHasTable(source, 'handoffs')
      ? Number((source.prepare('SELECT COUNT(*) AS count FROM handoffs').get() as { count: number | bigint }).count)
      : 0;
    const destinationExpected = sourceTable === 'event_outbox'
      ? expected + synthetic.length
      : expected + handoffCount + (refinementAdditions[destinationTable] ?? 0);
    if (Number(destinationCount.count) !== destinationExpected) throw new Error(`migration row-count mismatch for ${sourceTable}->${destinationTable}`);
  }
  const destinationEvents = destination.prepare(`SELECT sequence,event_id,workspace_path,event_type,aggregate_kind,aggregate_id,
    aggregate_revision,actor_json,provenance_json,payload_json,session_id,correlation_id,created_at,expires_at,schema_version,retention_class
    FROM event_outbox ORDER BY sequence`).all() as Array<Record<string, unknown>>;
  if (JSON.stringify(destinationEvents.map(row => row.event_id)) !== JSON.stringify(options.expectedEventIds)
    || JSON.stringify(destinationEvents.map(row => Number(row.sequence))) !== JSON.stringify(options.expectedEventSequences)) {
    throw new Error('migration event replay order does not match the previewed source order');
  }
  const highWater = destination.prepare("SELECT seq FROM sqlite_sequence WHERE name='event_outbox'").get() as { seq: number | bigint } | undefined;
  if (Number(highWater?.seq ?? 0) !== options.expectedEventHighWater) throw new Error('migration event sequence high-water mark does not match the previewed source');
  const sourceEvents = migrationHasTable(source, 'event_outbox') ? source.prepare(`SELECT event_id,workspace_path,event_type,aggregate_kind,aggregate_id,
    aggregate_revision,actor_json,provenance_json,payload_json,session_id,correlation_id,created_at,expires_at
    FROM event_outbox ORDER BY sequence`).all() as Array<Record<string, unknown>> : [];
  for (let index = 0; index < sourceEvents.length; index += 1) {
    const { sequence: _sequence, ...actual } = destinationEvents[index]!;
    const expected = { ...sourceEvents[index], schema_version: 1, retention_class: 'delivery' };
    if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error(`migration event replay mismatch at source sequence ${index + 1}`);
  }
  for (let index = 0; index < synthetic.length; index += 1) {
    const { sequence: _sequence, ...actual } = destinationEvents[sourceEvents.length + index]!;
    if (JSON.stringify(actual) !== JSON.stringify(synthetic[index])) throw new Error(`migration synthetic event replay mismatch at sequence ${index + 1}`);
  }
}

export function verifyLocalGitObjects(destination: DatabaseSync, root: string | undefined): { reachable: true | null; objects: number | null } {
  if (!root || !existsSync(root)) return { reachable: null, objects: null };
  const gitDir = join(root, 'repo.git');
  const oids = new Set<string>();
  if (migrationHasTable(destination, 'local_history_operations')) {
    const rows = destination.prepare('SELECT before_commit_oid,after_commit_oid FROM local_history_operations').all() as Array<Record<string, string | null>>;
    for (const row of rows) for (const oid of [row.before_commit_oid, row.after_commit_oid]) if (oid) oids.add(oid);
  }
  if (migrationHasTable(destination, 'local_history_versions')) {
    const rows = destination.prepare('SELECT before_oid,after_oid FROM local_history_versions').all() as Array<Record<string, string | null>>;
    for (const row of rows) for (const oid of [row.before_oid, row.after_oid]) if (oid) oids.add(oid);
  }
  for (const oid of oids) if (!/^[0-9a-f]{40}$/.test(oid) || !existsSync(join(gitDir, 'objects', oid.slice(0, 2), oid.slice(2)))) {
    throw new Error(`migration LocalGit object is missing or invalid: ${oid}`);
  }
  return { reachable: true, objects: oids.size };
}
