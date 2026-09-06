import { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';
import type { SQLInputValue } from 'node:sqlite';
import { SCHEMA_DDL, SCHEMA_INDEX_DDL } from './db-schema.js';
import { AWARENESS_APPLICATION_ID } from './storage-scope.js';

const LEGACY_COLUMNS: Record<string, readonly string[]> = {
  plans: ['plan_id', 'workspace_path', 'title', 'goal', 'name', 'objective', 'lead_agent_id', 'artifact', 'doc_dir', 'status', 'source_kind', 'source_key', 'rfc_path', 'rfc_revision', 'created_at', 'updated_at'],
  tasks: ['task_id', 'workspace_path', 'plan_id', 'title', 'file_path', 'paths_json', 'reasoning', 'acceptance', 'acceptance_criteria', 'created_by', 'check_command', 'status', 'priority', 'dependencies_json', 'agent_id', 'claimed_at', 'lease_expires_at', 'source_step_key', 'created_at', 'updated_at', 'done_at', 'completed_at', 'verified_at', 'verified_by', 'verification_message'],
  locks: ['workspace_path', 'file_path', 'agent_id', 'reason', 'acquired_at', 'expires_at'],
  work_presence: ['workspace_path', 'file_path', 'agent_id', 'reason', 'started_at', 'updated_at', 'expires_at'],
  memories: ['memory_id', 'workspace_path', 'label', 'text', 'tags_json', 'agent_id', 'task_context', 'observation', 'importance', 'state', 'superseded_by', 'artifact', 'repo', 'ref', 'file_tree_fingerprint', 'novelty_score', 'last_accessed_at', 'access_count', 'decay_half_life_days', 'failure_signature', 'valid_from', 'valid_to', 'expired_at', 'updated_at', 'scope_kind', 'source_digest', 'verified_at', 'secret_scan_status', 'embedding', 'embedding_model', 'created_at'],
  agents: ['agent_id', 'workspace_path', 'name', 'role', 'status', 'metadata_json', 'created_at', 'last_seen_at'],
  messages: ['message_id', 'workspace_path', 'from_agent_id', 'to_agent_id', 'topic', 'text', 'files_json', 'created_at'],
  message_receipts: ['message_id', 'agent_id', 'read_at'],
};

/** The immediately preceding canonical ledger used the unprefixed entity names. */
const PRIOR_CANONICAL_COLUMNS: Record<string, readonly string[]> = {
  plans: ['plan_id', 'name', 'objective', 'lead_agent_id', 'status', 'workspace_path', 'artifact', 'doc_dir', 'created_at', 'updated_at'],
  tasks: ['task_id', 'plan_id', 'title', 'reasoning', 'acceptance_criteria', 'status', 'priority', 'created_by', 'created_at', 'updated_at', 'completed_at'],
  locks: ['lock_id', 'file_path', 'run_id', 'acquired_at', 'expires_at'],
  work_presence: LEGACY_COLUMNS.work_presence!,
  memories: ['memory_id', 'agent_id', 'task_context', 'observation', 'importance', 'state', 'label', 'superseded_by', 'tags_json', 'workspace_path', 'artifact', 'repo', 'ref', 'file_tree_fingerprint', 'novelty_score', 'last_accessed_at', 'access_count', 'decay_half_life_days', 'failure_signature', 'valid_from', 'valid_to', 'expired_at', 'embedding', 'embedding_model', 'created_at', 'updated_at'],
  agents: ['agent_id', 'agent_name', 'workspace_path', 'artifact', 'context', 'registered_at', 'last_seen_at'],
  messages: LEGACY_COLUMNS.messages!,
  message_receipts: LEGACY_COLUMNS.message_receipts!,
};

export const LEGACY_TABLES = new Set(Object.keys(LEGACY_COLUMNS));
const SQLITE_AUXILIARY = /^(?:sqlite_|memories_fts(?:_|$))/;
type SqlScalar = Exclude<SQLInputValue, undefined>;

export interface DatabaseConsolidationOptions {
  /** Explicit actor to use only where historical rows omitted a required actor. */
  unattributedAgentId?: string;
}
export interface ConsolidationContractIssue {
  table: string;
  id: string;
  missing: readonly string[];
}
export class ConsolidationContractError extends Error {
  readonly code = 'INCOMPLETE_SOURCE_CONTRACT';
  constructor(readonly issues: readonly ConsolidationContractIssue[]) {
    const counts: Record<string, number> = {};
    for (const issue of issues) for (const field of issue.missing) counts[`${issue.table}.${field}`] = (counts[`${issue.table}.${field}`] ?? 0) + 1;
    super(`incomplete source contract: ${JSON.stringify({ counts })}`);
  }
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
export function actor(value: unknown, table: string, column: string, options: DatabaseConsolidationOptions, adopted: Set<string>): string {
  if (typeof value === 'string' && value.trim()) return value;
  const replacement = options.unattributedAgentId?.trim();
  if (!replacement) throw new Error(`unsupported source row: ${table}.${column} is required; provide unattributedAgentId to adopt unattributed rows`);
  adopted.add(replacement);
  return replacement;
}
export function jsonArray(value: unknown, table: string, column: string): unknown[] {
  const source = text(value, table, column);
  try {
    const parsed: unknown = JSON.parse(source);
    if (Array.isArray(parsed)) return parsed;
  } catch { /* use the explicit error below */ }
  throw new Error(`unsupported source row: ${table}.${column} must be a JSON array`);
}
function sameColumns(actual: readonly string[], expected: readonly string[]): boolean {
  return actual.length === expected.length && actual.every((name, index) => name === expected[index]);
}
export function sourceVariant(source: DatabaseSync, table: string): 'historical' | 'prior' {
  const actual = columns(source, table);
  if (sameColumns(actual, LEGACY_COLUMNS[table]!)) return 'historical';
  if (sameColumns(actual, PRIOR_CANONICAL_COLUMNS[table]!)) return 'prior';
  throw new Error(`unsupported source schema: malformed legacy table ${table}`);
}
export function assertExactLegacySchema(source: DatabaseSync): void {
  const names = new Set(tableNames(source));
  for (const table of Object.keys(LEGACY_COLUMNS)) {
    if (!names.has(table)) throw new Error(`unsupported source schema: missing legacy table ${table}`);
    sourceVariant(source, table);
  }
}
export function assertSupportedSourceTables(source: DatabaseSync): void {
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(SCHEMA_DDL);
    canonical.exec(SCHEMA_INDEX_DDL);
    const supported = new Set([...Object.keys(LEGACY_COLUMNS), ...tableNames(canonical), 'worker_lifecycle_events']);
    for (const name of tableNames(source)) {
      if (!supported.has(name) && !SQLITE_AUXILIARY.test(name)) throw new Error(`unsupported source schema: unknown table ${name}`);
    }
    const views = source.prepare("SELECT name FROM sqlite_master WHERE type = 'view'").all() as Array<{ name: string }>;
    if (views.length > 0) throw new Error(`unsupported source schema: views are not supported (${views.map(({ name }) => name).join(', ')})`);
  } finally {
    canonical.close();
  }
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
export function hasText(value: unknown): boolean { return typeof value === 'string' && value.trim().length > 0; }
export function assertConvertibleSourceRows(source: DatabaseSync, options: DatabaseConsolidationOptions): void {
  const issues: ConsolidationContractIssue[] = [];
  const needsActor = (value: unknown) => !hasText(value) && !hasText(options.unattributedAgentId);
  const planVariant = sourceVariant(source, 'plans');
  for (const row of source.prepare('SELECT * FROM plans ORDER BY plan_id').all() as Array<Record<string, unknown>>) {
    const missing: string[] = [];
    if (!hasText(row['name']) && !hasText(row['title'])) missing.push('name/title');
    if (!hasText(row['objective']) && !hasText(row['goal'])) missing.push('objective/goal');
    if (!hasText(row['doc_dir'])) missing.push('doc_dir');
    if (needsActor(row['lead_agent_id'])) missing.push('lead_agent_id');
    if (planVariant === 'prior' && !hasText(row['objective'])) missing.push('objective');
    if (missing.length) issues.push({ table: 'plans', id: String(row['plan_id']), missing });
  }
  const taskVariant = sourceVariant(source, 'tasks');
  for (const row of source.prepare('SELECT * FROM tasks ORDER BY task_id').all() as Array<Record<string, unknown>>) {
    const missing: string[] = [];
    if (!hasText(row['title'])) missing.push('title');
    if (!hasText(row['reasoning'])) missing.push('reasoning');
    if (!hasText(row['acceptance_criteria']) && !hasText(row['acceptance']) && !hasText(row['check_command'])) missing.push('acceptance_criteria/acceptance/check_command');
    if (needsActor(row['created_by']) && needsActor(row['agent_id'])) missing.push('created_by/agent_id');
    const active = row['status'] === 'CLAIMED' || row['status'] === 'IN_PROGRESS';
    if (taskVariant === 'historical' && active && !hasText(row['claimed_at'])) missing.push('claimed_at');
    if (taskVariant === 'historical' && active && !hasText(row['lease_expires_at'])) missing.push('lease_expires_at');
    if (missing.length) issues.push({ table: 'tasks', id: String(row['task_id']), missing });
  }
  for (const row of source.prepare('SELECT * FROM memories ORDER BY memory_id').all() as Array<Record<string, unknown>>) {
    const missing: string[] = [];
    if (needsActor(row['agent_id'])) missing.push('agent_id');
    if (!hasText(row['task_context']) && !hasText(row['text'])) missing.push('task_context/text');
    if (!hasText(row['observation']) && !hasText(row['text'])) missing.push('observation/text');
    if (missing.length) issues.push({ table: 'memories', id: String(row['memory_id']), missing });
  }
  for (const row of source.prepare('SELECT * FROM work_presence ORDER BY workspace_path, agent_id, file_path').all() as Array<Record<string, unknown>>) {
    issues.push({ table: 'work_presence', id: `${String(row['workspace_path'])}:${String(row['agent_id'])}:${String(row['file_path'])}`, missing: ['test_plan'] });
  }
  if (sourceVariant(source, 'locks') === 'historical') {
    for (const row of source.prepare('SELECT * FROM locks ORDER BY workspace_path, agent_id, file_path').all() as Array<Record<string, unknown>>) {
      issues.push({ table: 'locks', id: `${String(row['workspace_path'])}:${String(row['agent_id'])}:${String(row['file_path'])}`, missing: ['test_plan'] });
    }
  }
  if (issues.length) throw new ConsolidationContractError(issues);
}
export function assertNoCrossLedgerCollisions(source: DatabaseSync): void {
  const names = new Set(tableNames(source));
  const conflicts: Array<[string, string, string]> = [
    ['plans', 'awareness_plans', 'plan_id'], ['tasks', 'awareness_tasks', 'task_id'],
    ['memories', 'awareness_memories', 'memory_id'], ['messages', 'signals', 'message_id'],
  ];
  for (const [legacy, canonical, id] of conflicts) {
    if (!names.has(canonical)) continue;
    const canonicalId = legacy === 'messages' ? 'signal_id' : id;
    const row = source.prepare(`SELECT 1 FROM ${legacy} l JOIN ${canonical} c ON l.${id} = c.${canonicalId} LIMIT 1`).get();
    if (row) throw new Error(`source ID collision between ${legacy} and ${canonical}`);
  }
  if (names.has('awareness_agents')) {
    const row = source.prepare(`SELECT 1 FROM agents l JOIN awareness_agents c
      ON l.agent_id = c.agent_id AND l.workspace_path = c.workspace_path LIMIT 1`).get();
    if (row) throw new Error('source ID collision between agents and awareness_agents');
  }
}
export function copyCommonTables(source: DatabaseSync, destination: DatabaseSync): Record<string, number> {
  const sourceNames = new Set(tableNames(source));
  const result: Record<string, number> = {};
  for (const table of tableNames(destination)) {
    if (!sourceNames.has(table) || LEGACY_TABLES.has(table) || SQLITE_AUXILIARY.test(table)) continue;
    const destinationColumns = columns(destination, table);
    const sourceColumns = columns(source, table);
    if (destinationColumns.length === 0) continue;
    const destinationInfo = destination.prepare(`PRAGMA table_info(${JSON.stringify(table)})`).all() as Array<{ name: string; notnull: number; dflt_value: string | null }>;
    const extra = sourceColumns.filter((column) => !destinationColumns.includes(column));
    if (extra.length > 0) throw new Error(`unsupported source schema: ${table} has unmappable columns ${extra.join(', ')}`);
    for (const column of destinationInfo) {
      if (!sourceColumns.includes(column.name) && column.notnull !== 0 && column.dflt_value === null) {
        throw new Error(`unsupported source schema: ${table} lacks required column ${column.name}`);
      }
    }
    const selectedColumns = destinationColumns.filter((column) => sourceColumns.includes(column));
    if (selectedColumns.length === 0) throw new Error(`unsupported source schema: ${table} has no mappable columns`);
    const quoted = selectedColumns.map((column) => JSON.stringify(column)).join(', ');
    const rows = source.prepare(`SELECT ${quoted} FROM ${JSON.stringify(table)}`).all() as Array<Record<string, unknown>>;
    if (rows.length === 0) { result[table] = 0; continue; }
    const insert = destination.prepare(`INSERT INTO ${JSON.stringify(table)} (${quoted}) VALUES (${selectedColumns.map((column) => `@${column}`).join(', ')})`);
    for (const row of rows) {
      const values: Record<string, SQLInputValue> = {};
      for (const column of selectedColumns) {
        values[column] = scalar(row[column], table, column);
      }
      insert.run(values);
    }
    result[table] = rows.length;
  }
  return result;
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
