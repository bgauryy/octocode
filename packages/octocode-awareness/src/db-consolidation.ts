import { createHash } from 'node:crypto';
import { chmodSync, existsSync, linkSync, mkdtempSync, rmSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import { DatabaseSync } from '@octocodeai/agent-contracts/sqlite';
import type { SQLInputValue } from 'node:sqlite';
import { FTS_SCHEMA_DDL, SCHEMA_DDL, SCHEMA_INDEX_DDL } from './db-schema.js';
import { hasFts, rebuildFts } from './db-maintenance.js';
import { MEMORY_LABELS } from './schema/common.js';
import { AWARENESS_APPLICATION_ID } from './storage-scope.js';
import { WORKER_LIFECYCLE_DDL } from './worker-lifecycle-ledger.js';
import {
  actor,
  assertConvertibleSourceRows,
  assertExactLegacySchema,
  assertLogicalDestination,
  assertNoCrossLedgerCollisions,
  assertSupportedSourceTables,
  assertValidSource,
  copyCommonTables,
  hasText,
  jsonArray,
  nullableText,
  scalar,
  sourceVariant,
  tableNames,
  text,
} from './db-consolidation-validation.js';
import type { DatabaseConsolidationOptions } from './db-consolidation-validation.js';

export interface DatabaseConsolidationReport {
  sourcePath: string;
  destinationPath: string;
  copiedTables: Readonly<Record<string, number>>;
  adoptedAgentIds: readonly string[];
}


function copyLegacyMessages(source: DatabaseSync, destination: DatabaseSync): number {
  const rows = source.prepare('SELECT * FROM messages ORDER BY created_at, message_id').all() as Array<Record<string, unknown>>;
  const insert = destination.prepare(`INSERT INTO signals (signal_id, workspace_path, artifact, repo, ref, from_agent, to_agent, kind, subject, body, files_json, refs_json, thread_id, reply_to, importance, status, resolved_at, created_at)
    VALUES (@signal_id, @workspace_path, NULL, NULL, NULL, @from_agent, @to_agent, 'message', @subject, @body, @files_json, '[]', @thread_id, NULL, 5, 'open', NULL, @created_at)`);
  for (const row of rows) insert.run({
    signal_id: text(row['message_id'], 'messages', 'message_id'), workspace_path: text(row['workspace_path'], 'messages', 'workspace_path'), from_agent: text(row['from_agent_id'], 'messages', 'from_agent_id'),
    to_agent: nullableText(row['to_agent_id'], 'messages', 'to_agent_id'), subject: nullableText(row['topic'], 'messages', 'topic') || 'message',
    body: text(row['text'], 'messages', 'text'), files_json: JSON.stringify(jsonArray(row['files_json'], 'messages', 'files_json')), thread_id: text(row['message_id'], 'messages', 'message_id'), created_at: text(row['created_at'], 'messages', 'created_at'),
  });
  const receipts = source.prepare('SELECT * FROM message_receipts').all() as Array<Record<string, unknown>>;
  const read = destination.prepare('INSERT INTO signal_reads (signal_id, agent_id, read_at) VALUES (@signal_id, @agent_id, @read_at)');
  for (const row of receipts) read.run({ signal_id: text(row['message_id'], 'message_receipts', 'message_id'), agent_id: text(row['agent_id'], 'message_receipts', 'agent_id'), read_at: text(row['read_at'], 'message_receipts', 'read_at') });
  return rows.length;
}
function copyLegacyAgents(source: DatabaseSync, destination: DatabaseSync): number {
  const variant = sourceVariant(source, 'agents');
  const rows = source.prepare('SELECT * FROM agents').all() as Array<Record<string, unknown>>;
  const insert = destination.prepare(`INSERT INTO awareness_agents (agent_id, agent_name, workspace_path, artifact, context, role, status, metadata_json, registered_at, last_seen_at)
    VALUES (@agent_id, @agent_name, @workspace_path, @artifact, @context, @role, @status, @metadata_json, @registered_at, @last_seen_at)`);
  for (const row of rows) {
    if (variant === 'prior') {
      insert.run({ agent_id: text(row['agent_id'], 'agents', 'agent_id'), agent_name: nullableText(row['agent_name'], 'agents', 'agent_name') ?? '', workspace_path: text(row['workspace_path'], 'agents', 'workspace_path'),
        artifact: nullableText(row['artifact'], 'agents', 'artifact'), context: nullableText(row['context'], 'agents', 'context'), role: null, status: 'ACTIVE', metadata_json: '{}', registered_at: text(row['registered_at'], 'agents', 'registered_at'), last_seen_at: text(row['last_seen_at'], 'agents', 'last_seen_at') });
      continue;
    }
    const metadata = text(row['metadata_json'], 'agents', 'metadata_json');
    try { JSON.parse(metadata); } catch { throw new Error('unsupported source row: agents.metadata_json must be JSON'); }
    insert.run({ agent_id: text(row['agent_id'], 'agents', 'agent_id'), agent_name: nullableText(row['name'], 'agents', 'name') ?? '', workspace_path: text(row['workspace_path'], 'agents', 'workspace_path'),
      artifact: null, context: metadata === '{}' ? null : metadata, role: nullableText(row['role'], 'agents', 'role'), status: text(row['status'], 'agents', 'status'), metadata_json: metadata, registered_at: text(row['created_at'], 'agents', 'created_at'), last_seen_at: text(row['last_seen_at'], 'agents', 'last_seen_at') });
  }
  return rows.length;
}
function copyLegacyMemories(source: DatabaseSync, destination: DatabaseSync, options: DatabaseConsolidationOptions, adopted: Set<string>): number {
  const labels = new Set<string>(MEMORY_LABELS);
  const rows = source.prepare('SELECT * FROM memories').all() as Array<Record<string, unknown>>;
  const insert = destination.prepare(`INSERT INTO awareness_memories (memory_id, agent_id, task_context, observation, importance, state, label, superseded_by, tags_json, workspace_path, artifact, repo, ref, file_tree_fingerprint, novelty_score, last_accessed_at, access_count, decay_half_life_days, failure_signature, valid_from, valid_to, expired_at, scope_kind, source_digest, verified_at, secret_scan_status, embedding, embedding_model, created_at, updated_at)
    VALUES (@memory_id, @agent_id, @task_context, @observation, @importance, @state, @label, @superseded_by, @tags_json, @workspace_path, @artifact, @repo, @ref, @file_tree_fingerprint, @novelty_score, @last_accessed_at, @access_count, @decay_half_life_days, @failure_signature, @valid_from, @valid_to, @expired_at, @scope_kind, @source_digest, @verified_at, @secret_scan_status, @embedding, @embedding_model, @created_at, @updated_at)`);
  for (const row of rows) {
    const original = text(row['label'], 'memories', 'label');
    const tags = jsonArray(row['tags_json'], 'memories', 'tags_json');
    const label = labels.has(original) ? original : 'OTHER';
    if (label === 'OTHER' && original !== 'OTHER') tags.push(`legacy-label:${original}`);
    const taskContext = nullableText(row['task_context'], 'memories', 'task_context') ?? text(row['text'], 'memories', 'text');
    const observation = nullableText(row['observation'], 'memories', 'observation') ?? text(row['text'], 'memories', 'text');
    const values: Record<string, SQLInputValue> = {
      memory_id: text(row['memory_id'], 'memories', 'memory_id'), agent_id: actor(row['agent_id'], 'memories', 'agent_id', options, adopted), task_context: taskContext, observation,
      importance: row['importance'] === null ? 5 : scalar(row['importance'], 'memories', 'importance'), state: row['state'] === null ? 'ACTIVE' : scalar(row['state'], 'memories', 'state'), label,
      superseded_by: scalar(row['superseded_by'], 'memories', 'superseded_by'), tags_json: JSON.stringify(tags), workspace_path: nullableText(row['workspace_path'], 'memories', 'workspace_path'),
      artifact: scalar(row['artifact'], 'memories', 'artifact'), repo: scalar(row['repo'], 'memories', 'repo'), ref: scalar(row['ref'], 'memories', 'ref'), file_tree_fingerprint: scalar(row['file_tree_fingerprint'], 'memories', 'file_tree_fingerprint'),
      novelty_score: scalar(row['novelty_score'], 'memories', 'novelty_score'), last_accessed_at: scalar(row['last_accessed_at'], 'memories', 'last_accessed_at'), access_count: row['access_count'] === null ? 0 : scalar(row['access_count'], 'memories', 'access_count'),
      decay_half_life_days: scalar(row['decay_half_life_days'], 'memories', 'decay_half_life_days'), failure_signature: scalar(row['failure_signature'], 'memories', 'failure_signature'), valid_from: scalar(row['valid_from'], 'memories', 'valid_from'), valid_to: scalar(row['valid_to'], 'memories', 'valid_to'),
      expired_at: scalar(row['expired_at'], 'memories', 'expired_at'), scope_kind: scalar(row['scope_kind'] ?? null, 'memories', 'scope_kind'), source_digest: scalar(row['source_digest'] ?? null, 'memories', 'source_digest'), verified_at: scalar(row['verified_at'] ?? null, 'memories', 'verified_at'),
      secret_scan_status: scalar(row['secret_scan_status'] ?? null, 'memories', 'secret_scan_status'), embedding: scalar(row['embedding'], 'memories', 'embedding'), embedding_model: scalar(row['embedding_model'], 'memories', 'embedding_model'), created_at: text(row['created_at'], 'memories', 'created_at'), updated_at: scalar(row['updated_at'], 'memories', 'updated_at'),
    };
    insert.run(values);
  }
  return rows.length;
}

function legacyId(prefix: string, parts: readonly string[]): string {
  return `${prefix}_${createHash('sha256').update(parts.join('\u0000')).digest('hex').slice(0, 32)}`;
}
function legacyPlanStatus(value: unknown): string {
  switch (text(value, 'plans', 'status')) {
    case 'OPEN': return 'ACTIVE';
    case 'DONE': return 'COMPLETED';
    case 'ABANDONED': return 'CANCELLED';
    case 'DRAFT': case 'ACTIVE': case 'PAUSED': case 'COMPLETED': case 'CANCELLED': return String(value);
    default: throw new Error(`unsupported source row: plans.status ${String(value)}`);
  }
}
function nonemptyLegacyText(row: Record<string, unknown>, table: string, columns: readonly string[]): string {
  for (const column of columns) {
    const value = row[column];
    if (typeof value === 'string' && value.trim()) return value;
  }
  throw new Error(`unsupported source row: ${table}.${columns.join(' or ')} is required`);
}
function copyLegacyPlans(source: DatabaseSync, destination: DatabaseSync, options: DatabaseConsolidationOptions, adopted: Set<string>): number {
  const variant = sourceVariant(source, 'plans');
  const rows = source.prepare('SELECT * FROM plans ORDER BY created_at, plan_id').all() as Array<Record<string, unknown>>;
  const insert = destination.prepare(`INSERT INTO awareness_plans
    (plan_id, name, objective, lead_agent_id, status, workspace_path, artifact, doc_dir, source_kind, source_key, rfc_path, rfc_revision, created_at, updated_at)
    VALUES (@plan_id, @name, @objective, @lead_agent_id, @status, @workspace_path, @artifact, @doc_dir, @source_kind, @source_key, @rfc_path, @rfc_revision, @created_at, @updated_at)`);
  const member = destination.prepare('INSERT INTO plan_members(plan_id, agent_id, role, joined_at) VALUES (@plan_id, @agent_id, \'LEAD\', @joined_at) ON CONFLICT(plan_id, agent_id) DO NOTHING');
  for (const row of rows) {
    const planId = text(row['plan_id'], 'plans', 'plan_id');
    const lead = actor(row['lead_agent_id'], 'plans', 'lead_agent_id', options, adopted);
    insert.run({ plan_id: planId, name: nonemptyLegacyText(row, 'plans', ['name', 'title']), objective: nonemptyLegacyText(row, 'plans', ['objective', 'goal']), lead_agent_id: lead,
      status: variant === 'prior' ? text(row['status'], 'plans', 'status') : legacyPlanStatus(row['status']), workspace_path: text(row['workspace_path'], 'plans', 'workspace_path'), artifact: scalar(row['artifact'], 'plans', 'artifact'), doc_dir: text(row['doc_dir'], 'plans', 'doc_dir'),
      source_kind: scalar(row['source_kind'] ?? null, 'plans', 'source_kind'), source_key: scalar(row['source_key'] ?? null, 'plans', 'source_key'), rfc_path: scalar(row['rfc_path'] ?? null, 'plans', 'rfc_path'), rfc_revision: scalar(row['rfc_revision'] ?? null, 'plans', 'rfc_revision'),
      created_at: text(row['created_at'], 'plans', 'created_at'), updated_at: text(row['updated_at'], 'plans', 'updated_at') });
    member.run({ plan_id: planId, agent_id: lead, joined_at: text(row['created_at'], 'plans', 'created_at') });
  }
  return rows.length;
}

type ConvertedTask = { taskId: string; runId: string | null; status: string; agentId: string | null; workspacePath: string; createdAt: string; updatedAt: string; };
function taskStatus(row: Record<string, unknown>, variant: 'historical' | 'prior'): { status: string; runStatus: string | null } {
  if (variant === 'prior') return { status: text(row['status'], 'tasks', 'status'), runStatus: null };
  const old = text(row['status'], 'tasks', 'status');
  if (old === 'DONE') {
    const receivedVerification = hasText(row['verified_at']) && hasText(row['verified_by']) && hasText(row['verification_message']);
    return receivedVerification ? { status: 'DONE', runStatus: 'SUCCESS' } : { status: 'VERIFY', runStatus: 'PENDING' };
  }
  if (old === 'CLAIMED' || old === 'IN_PROGRESS') return { status: 'IN_PROGRESS', runStatus: 'ACTIVE' };
  if (old === 'VERIFY') return { status: 'VERIFY', runStatus: 'PENDING' };
  if (old === 'FAILED') return { status: 'FAILED', runStatus: 'FAILED' };
  if (old === 'OPEN' || old === 'BLOCKED' || old === 'CANCELLED') return { status: old, runStatus: null };
  throw new Error(`unsupported source row: tasks.status ${old}`);
}
function copyLegacyTasks(source: DatabaseSync, destination: DatabaseSync, options: DatabaseConsolidationOptions, adopted: Set<string>): ConvertedTask[] {
  const variant = sourceVariant(source, 'tasks');
  const rows = source.prepare('SELECT * FROM tasks ORDER BY created_at, task_id').all() as Array<Record<string, unknown>>;
  const taskInsert = destination.prepare(`INSERT INTO awareness_tasks
    (task_id, plan_id, title, reasoning, acceptance_criteria, source_step_key, check_command, status, priority, created_by, created_at, updated_at, completed_at)
    VALUES (@task_id, @plan_id, @title, @reasoning, @acceptance_criteria, @source_step_key, @check_command, @status, @priority, @created_by, @created_at, @updated_at, @completed_at)`);
  const pathInsert = destination.prepare('INSERT INTO task_paths(task_id, path, ordinal) VALUES (@task_id, @path, @ordinal)');
  const dependencyInsert = destination.prepare('INSERT INTO task_dependencies(task_id, depends_on_task_id, created_by, created_at) VALUES (@task_id, @depends_on_task_id, @created_by, @created_at)');
  const runInsert = destination.prepare(`INSERT INTO task_runs
    (run_id, task_id, origin, agent_id, session_id, rationale, test_plan, context_ref, status, workspace_path, artifact, created_at, updated_at)
    VALUES (@run_id, @task_id, 'TASK', @agent_id, NULL, @rationale, @test_plan, NULL, @status, @workspace_path, NULL, @created_at, @updated_at)`);
  const claimInsert = destination.prepare('INSERT INTO task_claims(task_id, run_id, agent_id, claimed_at, heartbeat_at, expires_at) VALUES (@task_id, @run_id, @agent_id, @claimed_at, @heartbeat_at, @expires_at)');
  const eventInsert = destination.prepare('INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at) VALUES (@event_id, @task_id, @run_id, @agent_id, @event_type, @message, @created_at)');
  const converted: ConvertedTask[] = [];
  for (const row of rows) {
    const taskId = text(row['task_id'], 'tasks', 'task_id');
    const state = taskStatus(row, variant);
    const agent = nullableText(row['agent_id'] ?? null, 'tasks', 'agent_id');
    const createdAt = text(row['created_at'], 'tasks', 'created_at');
    const updatedAt = text(row['updated_at'], 'tasks', 'updated_at');
    const createdBy = nullableText(row['created_by'], 'tasks', 'created_by') ?? (agent ?? actor(null, 'tasks', 'created_by', options, adopted));
    taskInsert.run({ task_id: taskId, plan_id: text(row['plan_id'], 'tasks', 'plan_id'), title: text(row['title'], 'tasks', 'title'),
      reasoning: nonemptyLegacyText(row, 'tasks', ['reasoning']), acceptance_criteria: nonemptyLegacyText(row, 'tasks', ['acceptance_criteria', 'acceptance', 'check_command']), source_step_key: scalar(row['source_step_key'] ?? null, 'tasks', 'source_step_key'), check_command: scalar(row['check_command'] ?? null, 'tasks', 'check_command'),
      status: state.status, priority: scalar(row['priority'], 'tasks', 'priority'), created_by: createdBy, created_at: createdAt, updated_at: updatedAt, completed_at: row['completed_at'] === null ? scalar(row['done_at'] ?? null, 'tasks', 'done_at') : scalar(row['completed_at'], 'tasks', 'completed_at') });
    if (variant === 'historical') {
      const paths = jsonArray(row['paths_json'], 'tasks', 'paths_json');
      const filePath = nullableText(row['file_path'], 'tasks', 'file_path');
      const allPaths = paths.length > 0 ? paths : filePath === null ? [] : [filePath];
      for (const [ordinal, path] of allPaths.entries()) pathInsert.run({ task_id: taskId, path: text(path, 'tasks.paths_json', String(ordinal)), ordinal });
      for (const dependency of jsonArray(row['dependencies_json'], 'tasks', 'dependencies_json')) dependencyInsert.run({ task_id: taskId, depends_on_task_id: text(dependency, 'tasks.dependencies_json', 'dependency'), created_by: createdBy, created_at: createdAt });
    }
    let runId: string | null = null;
    if (state.runStatus !== null) {
      const runAgent = agent ?? actor(null, 'tasks', 'agent_id', options, adopted);
      runId = legacyId('legacy_task', [taskId]);
      const runUpdatedAt = state.runStatus === 'SUCCESS' ? text(row['verified_at'], 'tasks', 'verified_at') : updatedAt;
      runInsert.run({ run_id: runId, task_id: taskId, agent_id: runAgent, rationale: nonemptyLegacyText(row, 'tasks', ['reasoning']), test_plan: nonemptyLegacyText(row, 'tasks', ['acceptance_criteria', 'acceptance', 'check_command']), status: state.runStatus, workspace_path: text(row['workspace_path'], 'tasks', 'workspace_path'), created_at: createdAt, updated_at: runUpdatedAt });
      if (state.runStatus === 'ACTIVE') {
        const claimedAt = text(row['claimed_at'], 'tasks', 'claimed_at');
        const expiresAt = text(row['lease_expires_at'], 'tasks', 'lease_expires_at');
        claimInsert.run({ task_id: taskId, run_id: runId, agent_id: runAgent, claimed_at: claimedAt, heartbeat_at: updatedAt, expires_at: expiresAt });
      }
      if (state.runStatus === 'SUCCESS') {
        const verifier = nullableText(row['verified_by'], 'tasks', 'verified_by');
        const message = nullableText(row['verification_message'], 'tasks', 'verification_message');
        if (verifier && message) eventInsert.run({ event_id: legacyId('legacy_verified', [taskId]), task_id: taskId, run_id: runId, agent_id: verifier, event_type: 'VERIFIED', message, created_at: text(row['verified_at'], 'tasks', 'verified_at') });
      }
    }
    const workspacePath = variant === 'historical'
      ? text(row['workspace_path'], 'tasks', 'workspace_path')
      : text((destination.prepare('SELECT workspace_path FROM awareness_plans WHERE plan_id = ?').get(text(row['plan_id'], 'tasks', 'plan_id')) as { workspace_path?: unknown } | undefined)?.workspace_path, 'awareness_plans', 'workspace_path');
    converted.push({ taskId, runId, status: state.status, agentId: agent, workspacePath, createdAt, updatedAt });
  }
  return converted;
}

function copyLegacyWorkAndLocks(source: DatabaseSync, destination: DatabaseSync): { workPresence: number; locks: number } {
  const workPresence = Number((source.prepare('SELECT COUNT(*) AS count FROM work_presence').get() as { count: number }).count);
  if (workPresence > 0) throw new Error('historical work presence requires an explicit test plan before conversion');
  const lockVariant = sourceVariant(source, 'locks');
  if (lockVariant === 'prior') {
    const rows = source.prepare('SELECT * FROM locks ORDER BY acquired_at, lock_id').all() as Array<Record<string, unknown>>;
    const direct = destination.prepare('INSERT INTO awareness_locks(lock_id, file_path, run_id, acquired_at, expires_at) VALUES (@lock_id, @file_path, @run_id, @acquired_at, @expires_at)');
    for (const row of rows) direct.run({ lock_id: text(row['lock_id'], 'locks', 'lock_id'), file_path: text(row['file_path'], 'locks', 'file_path'), run_id: text(row['run_id'], 'locks', 'run_id'), acquired_at: text(row['acquired_at'], 'locks', 'acquired_at'), expires_at: scalar(row['expires_at'], 'locks', 'expires_at') });
    return { workPresence, locks: rows.length };
  }
  const locks = Number((source.prepare('SELECT COUNT(*) AS count FROM locks').get() as { count: number }).count);
  if (locks > 0) throw new Error('historical locks require an explicit test plan before conversion');
  return { workPresence, locks };
}

/**
 * Converts an exact historical coordination database into a brand-new canonical
 * database. The source is opened read-only and is never a migration target.
 */
export function consolidateDatabase(sourcePath: string, destinationPath: string, options: DatabaseConsolidationOptions = {}): DatabaseConsolidationReport {
  if (!sourcePath || !destinationPath) throw new Error('source and destination paths are required');
  if (sourcePath === destinationPath) throw new Error('destination must differ from source');
  if (!existsSync(sourcePath)) throw new Error(`source database does not exist: ${sourcePath}`);
  if (existsSync(destinationPath)) throw new Error(`destination already exists: ${destinationPath}`);
  const temporaryDirectory = mkdtempSync(join(dirname(destinationPath), '.awareness-consolidation-'));
  chmodSync(temporaryDirectory, 0o700);
  const temporaryPath = join(temporaryDirectory, basename(destinationPath));
  let source: DatabaseSync | undefined;
  let destination: DatabaseSync | undefined;
  const adoptedAgentIds = new Set<string>();
  try {
    source = new DatabaseSync(sourcePath, { readOnly: true });
    source.exec('BEGIN');
    assertValidSource(source);
    assertExactLegacySchema(source);
    assertSupportedSourceTables(source);
    assertConvertibleSourceRows(source, options);
    assertNoCrossLedgerCollisions(source);
    destination = new DatabaseSync(temporaryPath);
    destination.exec('PRAGMA foreign_keys = OFF');
    destination.exec('BEGIN IMMEDIATE');
    destination.exec(SCHEMA_DDL);
    destination.exec(SCHEMA_INDEX_DDL);
    if (new Set(tableNames(source)).has('worker_lifecycle_events')) destination.exec(WORKER_LIFECYCLE_DDL);
    const copiedTables = copyCommonTables(source, destination);
    copiedTables.messages = copyLegacyMessages(source, destination);
    copiedTables.agents = copyLegacyAgents(source, destination);
    copiedTables.memories = copyLegacyMemories(source, destination, options, adoptedAgentIds);
    copiedTables.plans = copyLegacyPlans(source, destination, options, adoptedAgentIds);
    copiedTables.tasks = copyLegacyTasks(source, destination, options, adoptedAgentIds).length;
    const work = copyLegacyWorkAndLocks(source, destination);
    copiedTables.work_presence = work.workPresence;
    copiedTables.locks = work.locks;
    try { destination.exec(FTS_SCHEMA_DDL); } catch { /* FTS5 is optional in the embedded SQLite build. */ }
    if (hasFts(destination)) rebuildFts(destination);
    destination.exec(`PRAGMA application_id = ${AWARENESS_APPLICATION_ID}`);
    assertLogicalDestination(destination);
    const integrity = destination.prepare('PRAGMA integrity_check').get() as { integrity_check: string };
    if (integrity.integrity_check !== 'ok') throw new Error(`destination integrity check failed: ${integrity.integrity_check}`);
    const foreignKeys = destination.prepare('PRAGMA foreign_key_check').all();
    if (foreignKeys.length > 0) throw new Error('destination foreign key check failed');
    destination.exec('COMMIT');
    destination.exec('PRAGMA foreign_keys = ON');
    source.exec('COMMIT');
    destination.exec('PRAGMA wal_checkpoint(TRUNCATE)');
    destination.close();
    destination = undefined;
    try {
      linkSync(temporaryPath, destinationPath);
    } catch (error) {
      const detail = error as NodeJS.ErrnoException;
      if (detail.code === 'EEXIST') throw new Error(`destination already exists: ${destinationPath}`);
      throw error;
    }
    return { sourcePath, destinationPath, copiedTables, adoptedAgentIds: [...adoptedAgentIds] };
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
