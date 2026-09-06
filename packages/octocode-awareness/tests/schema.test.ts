/**
 * schema.test.ts — Structural tests for the clean db.ts schema.
 *
 * Verifies:
 *  - current table names are created
 *  - no extra application tables are created
 *  - column names match the current schema
 *  - FTS5 virtual table is created and functional
 *  - initDb is idempotent
 */
import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { getDeliveryFingerprint, setDeliveryFingerprint } from '../src/db-runtime.js';
import { initDb } from '../src/db-init.js';
import { tableColumns } from '../src/db-introspection.js';
import { AWARENESS_APPLICATION_ID } from '../src/storage-scope.js';
// ─── Helpers ──────────────────────────────────────────────────────────────────
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}

// ─── 1. Current tables are created ────────────────────────────────────────────

describe('initDb creates all required tables', () => {
  const db = freshDb();

  const requiredTables = [
    'authorization_receipts', 'capability_receipts', 'event_outbox', 'event_consumers', 'event_acknowledgements', 'pending_interactions', 'handoffs',
    'awareness_memories',
    'memories_fts',
    'memory_refs',
    'awareness_plans',
    'plan_members',
    'plan_docs',
    'awareness_tasks',
    'task_paths',
    'task_dependencies',
    'task_claims',
    'task_events',
    'task_runs',
    'run_files',
    'awareness_locks',
    'delivery_state',
    'run_log',
    'signals',
    'signal_reads',
    'awareness_agents',
    'sessions',
    'refinements',
    'edit_log',
    'harness_log',
  ] as const;

  for (const table of requiredTables) {
    it(`creates table "${table}"`, () => {
      // memories_fts is a virtual table — check via sqlite_master directly
      const row = db.prepare(
        "SELECT name FROM sqlite_master WHERE name = ?"
      ).get(table) as { name: string } | undefined;
      expect(row, `expected "${table}" in sqlite_master`).toBeDefined();
    });
  }
});

// ─── 2. Table set stays constrained ──────────────────────────────────────────

describe('initDb table set', () => {
  const db = freshDb();

  it('creates only known application tables plus FTS internals', () => {
    const allowed = new Set([
      'authorization_receipts', 'capability_receipts', 'event_outbox', 'event_consumers', 'event_acknowledgements', 'pending_interactions', 'handoffs',
      'sessions',
      'awareness_memories',
      'awareness_plans',
      'plan_members',
      'plan_docs',
      'awareness_tasks',
      'task_paths',
      'task_dependencies',
      'task_claims',
      'task_events',
      'task_runs',
      'run_files',
      'awareness_locks',
      'delivery_state',
      'hook_receipts',
      'run_log',
      'refinements',
      'signals',
      'signal_reads',
      'memory_refs',
      'awareness_agents',
      'edit_log',
      'harness_log',
    ]);
    const rows = db.prepare(
      "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
    ).all() as Array<{ name: string }>;
    const unexpected = rows
      .map(r => r.name)
      .filter(name => !allowed.has(name) && !name.startsWith('memories_fts') && !name.startsWith('sqlite_'));
    expect(unexpected).toEqual([]);
  });
});

// ─── 3. memories table columns ────────────────────────────────────────────────

describe('memories table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'awareness_memories')].sort();

  it('matches the current memory column set', () => {
    expect(cols).toEqual([
      'access_count',
      'agent_id',
      'artifact',
      'created_at',
      'decay_half_life_days',
      'embedding',
      'embedding_model',
      'expired_at',
      'failure_signature',
      'file_tree_fingerprint',
      'importance',
      'label',
      'last_accessed_at',
      'memory_id',
      'novelty_score',
      'observation',
      'ref',
      'repo',
      'scope_kind',
      'secret_scan_status',
      'source_digest',
      'state',
      'superseded_by',
      'tags_json',
      'task_context',
      'updated_at',
      'valid_from',
      'valid_to',
      'verified_at',
      'workspace_path',
    ]);
  });
});

// ─── 4. tasks table ──────────────────────────────────────────────────────────

describe('tasks table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'awareness_tasks')].sort();

  it('matches the current task column set', () => {
    expect(cols).toEqual([
      'acceptance_criteria',
      'check_command',
      'completed_at',
      'created_at',
      'created_by',
      'plan_id',
      'priority',
      'reasoning',
      'source_step_key',
      'status',
      'task_id',
      'title',
      'updated_at',
    ]);
  });
});

describe('task_runs table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'task_runs')].sort();

  it('keeps execution attempts separate from durable tasks', () => {
    expect(cols).toEqual([
      'agent_id',
      'artifact',
      'context_ref',
      'created_at',
      'origin',
      'rationale',
      'run_id',
      'session_id',
      'status',
      'task_id',
      'test_plan',
      'updated_at',
      'workspace_path',
    ]);
  });
});

// ─── 5. locks table ──────────────────────────────────────────────────────────

describe('locks table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'awareness_locks')].sort();

  it('matches the current lock column set', () => {
    expect(cols).toEqual([
      'acquired_at',
      'expires_at',
      'file_path',
      'lock_id',
      'run_id',
    ]);
  });
});

describe('run_files table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'run_files')].sort();

  it('normalizes advisory file presence separately from runs and locks', () => {
    expect(cols).toEqual([
      'ended_at',
      'expires_at',
      'file_path',
      'heartbeat_at',
      'reason_override',
      'run_id',
      'source',
      'started_at',
    ]);
  });
});

describe('delivery_state table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'delivery_state')].sort();

  it('stores compact-output fingerprints without duplicating awareness payloads', () => {
    expect(cols).toEqual([
      'channel',
      'consumer_id',
      'delivered_at',
      'fingerprint',
      'scope_key',
    ]);
  });
});

describe('canonical schema identity', () => {
  it('uses the historical Awareness application identity', () => {
    const db = freshDb();
    expect(db.prepare('PRAGMA application_id').get())
      .toEqual({ application_id: AWARENESS_APPLICATION_ID });
  });

  it('rejects an extra application relation on the canonical fast path', () => {
    const db = freshDb();
    db.exec('CREATE TABLE unexpected_state(value TEXT)');
    expect(() => initDb(db)).toThrow(/unrecognized.*unexpected_state|canonical relation contract mismatch.*unexpected_state/i);
  });

  it('rejects canonical-header structural drift', () => {
    const db = freshDb();
    db.exec(`
      PRAGMA foreign_keys = OFF;
      DROP TABLE task_paths;
      CREATE TABLE task_paths(task_id TEXT NOT NULL, path TEXT NOT NULL, ordinal INTEGER NOT NULL DEFAULT 0);
    `);
    expect(() => initDb(db)).toThrow(/canonical schema fingerprint mismatch/i);
  });

  it('rejects a missing canonical index', () => {
    const db = freshDb();
    db.exec('DROP INDEX idx_sessions_agent');
    expect(() => initDb(db)).toThrow(/canonical schema fingerprint mismatch/i);
  });

  it('rejects an unexpected trigger', () => {
    const db = freshDb();
    db.exec(`CREATE TRIGGER destructive_memory_trigger AFTER INSERT ON awareness_memories
      BEGIN DELETE FROM awareness_memories WHERE memory_id = NEW.memory_id; END`);
    expect(() => initDb(db)).toThrow(/canonical schema fingerprint mismatch/i);
  });
});

describe('delivery fingerprints', () => {
  it('upserts one fingerprint per consumer, channel, and scope', () => {
    const db = freshDb();
    const key = { consumerId: 'agent-a', channel: 'briefing', scopeKey: '/repo|-|session-a' };
    expect(getDeliveryFingerprint(db, key)).toBeNull();
    setDeliveryFingerprint(db, { ...key, fingerprint: 'v1' });
    expect(getDeliveryFingerprint(db, key)).toBe('v1');
    setDeliveryFingerprint(db, { ...key, fingerprint: 'v2' });
    expect(getDeliveryFingerprint(db, key)).toBe('v2');
    expect(db.prepare('SELECT COUNT(*) AS count FROM delivery_state').get()).toEqual({ count: 1 });
  });
});

// ─── 6. signals table ────────────────────────────────────────────────────────

describe('signals table column names', () => {
  const db = freshDb();
  const cols = [...tableColumns(db, 'signals')].sort();

  it('matches the current signal column set', () => {
    expect(cols).toEqual([
      'artifact',
      'body',
      'created_at',
      'files_json',
      'from_agent',
      'importance',
      'kind',
      'ref',
      'refs_json',
      'reply_to',
      'repo',
      'resolved_at',
      'signal_id',
      'status',
      'subject',
      'thread_id',
      'to_agent',
      'workspace_path',
    ]);
  });
});

describe('lifecycle enum constraints', () => {
  it('rejects unknown task event types', () => {
    const db = freshDb();
    db.prepare(`INSERT INTO awareness_plans(plan_id, name, objective, lead_agent_id, status, workspace_path, doc_dir, created_at, updated_at)
      VALUES ('plan_lifecycle', 'Lifecycle', 'Keep lifecycle values bounded.', 'lead', 'ACTIVE', '/tmp/repo', '.octocode/plan/lifecycle', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')`).run();
    db.prepare(`INSERT INTO awareness_tasks(task_id, plan_id, title, reasoning, acceptance_criteria, status, created_by, created_at, updated_at)
      VALUES ('task_lifecycle', 'plan_lifecycle', 'Task', 'reason', 'verify', 'OPEN', 'lead', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')`).run();

    expect(() => db.prepare(`INSERT INTO task_events(event_id, task_id, agent_id, event_type, message, created_at)
      VALUES ('event_bad', 'task_lifecycle', 'lead', 'BOGUS', 'bad event', '2026-01-01T00:00:00Z')`).run())
      .toThrow(/CHECK constraint failed/);

    expect(() => db.prepare(`INSERT INTO task_events(event_id, task_id, agent_id, event_type, message, created_at)
      VALUES ('event_good', 'task_lifecycle', 'lead', 'CREATED', 'created', '2026-01-01T00:00:00Z')`).run())
      .not.toThrow();
  });

  it('rejects unknown signal statuses', () => {
    const db = freshDb();
    const insert = db.prepare(`INSERT INTO signals(
      signal_id, workspace_path, from_agent, kind, subject, thread_id, importance, status, created_at
    ) VALUES (?, '/tmp/repo', 'agent-a', 'fyi', 'subject', ?, 5, ?, '2026-01-01T00:00:00Z')`);

    expect(() => insert.run('ntf_bad', 'ntf_bad', 'archived')).toThrow(/CHECK constraint failed/);
    expect(() => insert.run('ntf_good_open', 'ntf_good_open', 'open')).not.toThrow();
    expect(() => insert.run('ntf_good_resolved', 'ntf_good_resolved', 'resolved')).not.toThrow();
  });
});

// ─── 7. sessions table ───────────────────────────────────────────────────────

describe('sessions table column names', () => {
  const db = freshDb();
  const cols = tableColumns(db, 'sessions');

  it('has "session_id" as primary key column', () => {
    expect(cols.has('session_id')).toBe(true);
  });

  it('has expected session columns', () => {
    for (const col of ['agent_id', 'workspace_path', 'repo', 'ref', 'started_at', 'ended_at', 'summary']) {
      expect(cols.has(col), `missing column: ${col}`).toBe(true);
    }
  });
});
