import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { DatabaseSync } from '@octocodeai/octocode-shared/sqlite';
import { afterEach, describe, expect, it } from 'vitest';
import { consolidateDatabase } from '../src/db-consolidation.js';
import { ConsolidationContractError } from '../src/db-consolidation-validation.js';
import { SCHEMA_DDL, SCHEMA_INDEX_DDL } from '../src/db-schema.js';
import { AWARENESS_APPLICATION_ID } from '../src/storage-scope.js';
import { assertLogicalDestination } from '../src/db-consolidation-validation.js';

const LEGACY: Record<string, readonly string[]> = {
  plans: ['plan_id', 'workspace_path', 'title', 'goal', 'name', 'objective', 'lead_agent_id', 'artifact', 'doc_dir', 'status', 'source_kind', 'source_key', 'rfc_path', 'rfc_revision', 'created_at', 'updated_at'],
  tasks: ['task_id', 'workspace_path', 'plan_id', 'title', 'file_path', 'paths_json', 'reasoning', 'acceptance', 'acceptance_criteria', 'created_by', 'check_command', 'status', 'priority', 'dependencies_json', 'agent_id', 'claimed_at', 'lease_expires_at', 'source_step_key', 'created_at', 'updated_at', 'done_at', 'completed_at', 'verified_at', 'verified_by', 'verification_message'],
  locks: ['workspace_path', 'file_path', 'agent_id', 'reason', 'acquired_at', 'expires_at'],
  work_presence: ['workspace_path', 'file_path', 'agent_id', 'reason', 'started_at', 'updated_at', 'expires_at'],
  memories: ['memory_id', 'workspace_path', 'label', 'text', 'tags_json', 'agent_id', 'task_context', 'observation', 'importance', 'state', 'superseded_by', 'artifact', 'repo', 'ref', 'file_tree_fingerprint', 'novelty_score', 'last_accessed_at', 'access_count', 'decay_half_life_days', 'failure_signature', 'valid_from', 'valid_to', 'expired_at', 'updated_at', 'scope_kind', 'source_digest', 'verified_at', 'secret_scan_status', 'embedding', 'embedding_model', 'created_at'],
  agents: ['agent_id', 'workspace_path', 'name', 'role', 'status', 'metadata_json', 'created_at', 'last_seen_at'],
  messages: ['message_id', 'workspace_path', 'from_agent_id', 'to_agent_id', 'topic', 'text', 'files_json', 'created_at'],
  message_receipts: ['message_id', 'agent_id', 'read_at'],
};

const cleanups: string[] = [];
afterEach(() => { for (const path of cleanups.splice(0)) rmSync(path, { recursive: true, force: true }); });

function fixture(): { root: string; source: string; destination: string } {
  const root = mkdtempSync(join(tmpdir(), 'awareness-consolidation-'));
  cleanups.push(root);
  const source = join(root, 'legacy.sqlite3');
  const db = new DatabaseSync(source);
  for (const [table, columns] of Object.entries(LEGACY)) {
    db.exec(`CREATE TABLE ${table} (${columns.map((column) => `${column} ${column === 'priority' || column === 'importance' || column === 'access_count' ? 'INTEGER' : 'TEXT'}`).join(', ')})`);
  }
  const at = '2026-01-01T00:00:00Z';
  db.prepare(`INSERT INTO plans VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
    .run('plan-1', '/repo', 'Plan title', 'Plan goal', null, null, 'agent-a', null, 'docs', 'ACTIVE', null, null, null, null, at, at);
  db.prepare(`INSERT INTO tasks VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
    .run('task-verified', '/repo', 'plan-1', 'Verify task', 'src/a.ts', '["src/a.ts"]', 'reason', 'accept', null, 'agent-a', 'node test', 'DONE', 2, '[]', 'agent-a', null, null, null, at, at, at, at, '2026-01-01T00:01:00Z', 'reviewer', 'verified');
  db.prepare(`INSERT INTO tasks VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
    .run('task-active', '/repo', 'plan-1', 'Active task', 'src/b.ts', '[]', 'reason active', 'accept active', null, 'agent-a', null, 'IN_PROGRESS', 1, '["task-verified"]', 'agent-a', at, '2026-01-01T01:00:00Z', null, at, at, null, null, null, null, null);
  db.prepare(`INSERT INTO agents VALUES (?, ?, ?, ?, ?, ?, ?, ?)`)
    .run('agent-a', '/repo', 'Agent A', 'developer', 'ACTIVE', '{"team":"core"}', at, at);
  db.prepare(`INSERT INTO memories VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
    .run('memory-1', '/repo', 'CUSTOM_LABEL', 'Remember this', '["tag"]', 'agent-a', null, null, 8, 'ACTIVE', null, null, null, null, null, null, null, 0, null, null, null, null, null, null, null, null, null, null, null, null, at);
  db.prepare(`INSERT INTO messages VALUES (?, ?, ?, ?, ?, ?, ?, ?)`)
    .run('message-1', '/repo', 'agent-a', 'agent-b', null, 'Hello', '["src/a.ts"]', at);
  db.prepare(`INSERT INTO message_receipts VALUES (?, ?, ?)`)
    .run('message-1', 'agent-b', at);
  db.close();
  return { root, source, destination: join(root, 'canonical.sqlite3') };
}

function priorLedgerFixture(): { source: string; destination: string } {
  const result = fixture();
  const db = new DatabaseSync(result.source);
  for (const table of ['plans', 'tasks', 'locks', 'memories', 'agents']) db.exec(`DROP TABLE ${table}`);
  db.exec(`
    CREATE TABLE plans (plan_id TEXT, name TEXT, objective TEXT, lead_agent_id TEXT, status TEXT, workspace_path TEXT, artifact TEXT, doc_dir TEXT, created_at TEXT, updated_at TEXT);
    CREATE TABLE tasks (task_id TEXT, plan_id TEXT, title TEXT, reasoning TEXT, acceptance_criteria TEXT, status TEXT, priority INTEGER, created_by TEXT, created_at TEXT, updated_at TEXT, completed_at TEXT);
    CREATE TABLE locks (lock_id TEXT, file_path TEXT, run_id TEXT, acquired_at TEXT, expires_at TEXT);
    CREATE TABLE memories (memory_id TEXT, agent_id TEXT, task_context TEXT, observation TEXT, importance INTEGER, state TEXT, label TEXT, superseded_by TEXT, tags_json TEXT, workspace_path TEXT, artifact TEXT, repo TEXT, ref TEXT, file_tree_fingerprint TEXT, novelty_score TEXT, last_accessed_at TEXT, access_count INTEGER, decay_half_life_days TEXT, failure_signature TEXT, valid_from TEXT, valid_to TEXT, expired_at TEXT, embedding TEXT, embedding_model TEXT, created_at TEXT, updated_at TEXT);
    CREATE TABLE agents (agent_id TEXT, agent_name TEXT, workspace_path TEXT, artifact TEXT, context TEXT, registered_at TEXT, last_seen_at TEXT);
  `);
  const at = '2026-02-01T00:00:00Z';
  db.prepare('INSERT INTO plans VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)').run('prior-plan', 'Prior plan', 'Prior objective', 'agent-prior', 'ACTIVE', '/prior', null, 'docs', at, at);
  db.prepare('INSERT INTO tasks VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)').run('prior-task', 'prior-plan', 'Prior task', 'why', 'accept', 'OPEN', 0, 'agent-prior', at, at, null);
  db.prepare('INSERT INTO agents VALUES (?, ?, ?, ?, ?, ?, ?)').run('agent-prior', '', '/prior', 'prior-artifact', 'prior context', at, at);
  db.prepare('INSERT INTO memories VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)').run('prior-memory', 'agent-prior', 'context', 'observation', 5, 'ACTIVE', 'DOCS', null, '[]', null, null, null, null, null, null, null, 0, null, null, null, null, null, null, null, at, at);
  db.close();
  return { source: result.source, destination: result.destination };
}

describe('consolidateDatabase', () => {
  it('leaves the legacy source intact and converts messages, memories, tasks, work, and locks', () => {
    const { source, destination } = fixture();
    const before = readFileSync(source);
    const report = consolidateDatabase(source, destination);
    expect(readFileSync(source)).toEqual(before);
    expect(report.copiedTables).toMatchObject({ messages: 1, agents: 1, memories: 1, plans: 1, tasks: 2, work_presence: 0, locks: 0 });
    const db = new DatabaseSync(destination, { readOnly: true });
    expect(db.prepare('SELECT kind, subject, body, thread_id FROM signals WHERE signal_id = ?').get('message-1')).toEqual({ kind: 'message', subject: 'message', body: 'Hello', thread_id: 'message-1' });
    expect(db.prepare('SELECT agent_id FROM signal_reads WHERE signal_id = ?').get('message-1')).toEqual({ agent_id: 'agent-b' });
    expect(db.prepare('SELECT label, task_context, observation, tags_json FROM awareness_memories WHERE memory_id = ?').get('memory-1')).toEqual({ label: 'OTHER', task_context: 'Remember this', observation: 'Remember this', tags_json: '["tag","legacy-label:CUSTOM_LABEL"]' });
    expect(db.prepare('SELECT status FROM awareness_tasks WHERE task_id = ?').get('task-verified')).toEqual({ status: 'DONE' });
    expect(db.prepare('SELECT status, updated_at FROM task_runs WHERE task_id = ?').get('task-verified')).toEqual({ status: 'SUCCESS', updated_at: '2026-01-01T00:01:00Z' });
    expect(db.prepare('SELECT status FROM awareness_tasks WHERE task_id = ?').get('task-active')).toEqual({ status: 'IN_PROGRESS' });
    expect(db.prepare('SELECT status FROM task_runs WHERE task_id = ?').get('task-active')).toEqual({ status: 'ACTIVE' });
    expect(db.prepare('SELECT COUNT(*) AS count FROM task_claims').get()).toEqual({ count: 1 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_locks').get()).toEqual({ count: 0 });
    expect(db.prepare('PRAGMA application_id').get()).toEqual({ application_id: AWARENESS_APPLICATION_ID });
    expect(db.prepare("SELECT 1 AS present FROM sqlite_master WHERE name = 'memories_fts'").get()).toEqual({ present: 1 });
    db.close();
  });

  it('refuses an existing destination without mutating it', () => {
    const { source, destination } = fixture();
    const destinationDb = new DatabaseSync(destination);
    destinationDb.exec('CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES (\'keep\')');
    destinationDb.close();
    expect(() => consolidateDatabase(source, destination)).toThrow('destination already exists');
    const check = new DatabaseSync(destination, { readOnly: true });
    expect(check.prepare('SELECT value FROM sentinel').get()).toEqual({ value: 'keep' });
    check.close();
  });

  it('rejects malformed and foreign source schemas without leaving a destination', () => {
    const { source, destination } = fixture();
    const db = new DatabaseSync(source);
    db.exec('CREATE TABLE foreign_data(value TEXT)');
    db.close();
    expect(() => consolidateDatabase(source, destination)).toThrow('unknown table foreign_data');
    expect(existsSync(destination)).toBe(false);
  });

  it('rejects duplicate IDs across the old and canonical ledgers', () => {
    const { source, destination } = fixture();
    const db = new DatabaseSync(source);
    db.exec('CREATE TABLE awareness_plans(plan_id TEXT)');
    db.prepare('INSERT INTO awareness_plans VALUES (?)').run('plan-1');
    db.close();
    expect(() => consolidateDatabase(source, destination)).toThrow('source ID collision between plans and awareness_plans');
    expect(existsSync(destination)).toBe(false);
  });

  it('rejects historical work and locks without a recorded test plan', () => {
    const { source, destination } = fixture();
    const db = new DatabaseSync(source);
    const at = '2026-01-01T00:00:00Z';
    db.prepare('INSERT INTO work_presence VALUES (?, ?, ?, ?, ?, ?, ?)').run('/repo', 'src/work.ts', 'agent-a', 'editing', at, at, '2026-01-01T01:00:00Z');
    db.prepare('INSERT INTO locks VALUES (?, ?, ?, ?, ?, ?)').run('/repo', 'src/work.ts', 'agent-a', 'editing', at, '2026-01-01T01:00:00Z');
    db.close();
    expect(() => consolidateDatabase(source, destination)).toThrow(ConsolidationContractError);
    try { consolidateDatabase(source, destination); } catch (error) {
      expect(error).toBeInstanceOf(ConsolidationContractError);
      expect((error as ConsolidationContractError).issues).toEqual(expect.arrayContaining([
        expect.objectContaining({ table: 'work_presence', missing: ['test_plan'] }),
        expect.objectContaining({ table: 'locks', missing: ['test_plan'] }),
      ]));
    }
    expect(existsSync(destination)).toBe(false);
  });

  it('rejects a cyclic historical dependency and leaves its source and destination untouched', () => {
    const { source, destination } = fixture();
    const before = readFileSync(source);
    const db = new DatabaseSync(source);
    db.prepare('UPDATE tasks SET dependencies_json = ? WHERE task_id = ?').run('["task-active"]', 'task-verified');
    db.close();
    const mutated = readFileSync(source);
    expect(() => consolidateDatabase(source, destination)).toThrow('cyclic dependency at');
    expect(readFileSync(source)).toEqual(mutated);
    expect(readFileSync(source)).not.toEqual(before);
    expect(existsSync(destination)).toBe(false);
  });

  it('converts the immediately-prior unprefixed canonical entity ledger', () => {
    const { source, destination } = priorLedgerFixture();
    consolidateDatabase(source, destination);
    const db = new DatabaseSync(destination, { readOnly: true });
    expect(db.prepare('SELECT status, workspace_path FROM awareness_plans WHERE plan_id = ?').get('prior-plan')).toEqual({ status: 'ACTIVE', workspace_path: '/prior' });
    expect(db.prepare('SELECT status, created_by FROM awareness_tasks WHERE task_id = ?').get('prior-task')).toEqual({ status: 'OPEN', created_by: 'agent-prior' });
    expect(db.prepare('SELECT agent_name, artifact, context FROM awareness_agents WHERE agent_id = ?').get('agent-prior')).toEqual({ agent_name: '', artifact: 'prior-artifact', context: 'prior context' });
    expect(db.prepare('SELECT task_context, observation, workspace_path FROM awareness_memories WHERE memory_id = ?').get('prior-memory')).toEqual({ task_context: 'context', observation: 'observation', workspace_path: null });
    db.close();
  });

  it('keeps a historical completion pending when its verification receipt is incomplete', () => {
    const { source, destination } = fixture();
    const db = new DatabaseSync(source);
    db.prepare('UPDATE tasks SET verified_by = NULL, verification_message = NULL WHERE task_id = ?').run('task-verified');
    db.close();
    consolidateDatabase(source, destination);
    const converted = new DatabaseSync(destination, { readOnly: true });
    expect(converted.prepare('SELECT status FROM awareness_tasks WHERE task_id = ?').get('task-verified')).toEqual({ status: 'VERIFY' });
    expect(converted.prepare('SELECT status FROM task_runs WHERE task_id = ?').get('task-verified')).toEqual({ status: 'PENDING' });
    expect(converted.prepare("SELECT COUNT(*) AS count FROM task_events WHERE event_type = 'VERIFIED'").get()).toEqual({ count: 0 });
    converted.close();
  });

  it('preserves non-conflicting rows from both ledgers', () => {
    const { source, destination } = fixture();
    const db = new DatabaseSync(source);
    db.exec(SCHEMA_DDL);
    db.exec(SCHEMA_INDEX_DDL);
    db.prepare(`INSERT INTO awareness_memories(memory_id, agent_id, task_context, observation, importance, created_at)
      VALUES (?, ?, ?, ?, ?, ?)`)
      .run('canonical-memory', 'agent-a', 'canonical context', 'canonical observation', 6, '2026-01-01T00:00:00Z');
    db.close();
    consolidateDatabase(source, destination);
    const destinationDb = new DatabaseSync(destination, { readOnly: true });
    expect(destinationDb.prepare('SELECT COUNT(*) AS count FROM awareness_memories').get()).toEqual({ count: 2 });
    expect(destinationDb.prepare('SELECT observation FROM awareness_memories WHERE memory_id = ?').get('canonical-memory')).toEqual({ observation: 'canonical observation' });
    destinationDb.close();
  });

  it('detects cross-plan dependencies, claim/run disagreement, and task-run workspace disagreement', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(SCHEMA_DDL);
    db.exec(SCHEMA_INDEX_DDL);
    const at = '2026-01-01T00:00:00Z';
    db.prepare('INSERT INTO awareness_plans VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)').run('p1', 'p1', 'o', 'a', 'ACTIVE', '/one', null, 'docs', null, null, null, null, at, at);
    db.prepare('INSERT INTO awareness_plans VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)').run('p2', 'p2', 'o', 'b', 'ACTIVE', '/two', null, 'docs', null, null, null, null, at, at);
    db.prepare('INSERT INTO awareness_tasks VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)').run('t1', 'p1', 't1', 'reason', 'accept', null, null, 'IN_PROGRESS', 0, 'a', at, at, null);
    db.prepare('INSERT INTO awareness_tasks VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)').run('t2', 'p2', 't2', 'reason', 'accept', null, null, 'OPEN', 0, 'b', at, at, null);
    db.prepare('INSERT INTO task_dependencies VALUES (?, ?, ?, ?)').run('t1', 't2', 'a', at);
    db.prepare('INSERT INTO task_runs VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)').run('r1', 't2', 'TASK', 'b', null, 'reason', 'accept', null, 'ACTIVE', '/wrong', null, at, at);
    db.prepare('INSERT INTO task_claims VALUES (?, ?, ?, ?, ?, ?)').run('t1', 'r1', 'a', at, at, '2026-01-01T01:00:00Z');
    expect(() => assertLogicalDestination(db)).toThrow('cross-plan dependency t1->t2; claim/run mismatch t1/r1; run workspace mismatch r1');
    db.close();
  });
});
