import { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import { initDb } from '../src/db-init.js';
import { AWARENESS_APPLICATION_ID } from '../src/storage-scope.js';

const ADVANCED_TABLES = [
  'authorization_receipts', 'capability_receipts', 'event_outbox', 'event_consumers', 'event_acknowledgements', 'pending_interactions', 'handoffs',
  'awareness_agents', 'delivery_state', 'edit_log', 'harness_log', 'hook_receipts',
  'awareness_locks', 'awareness_memories', 'memory_refs', 'plan_docs', 'plan_members', 'awareness_plans',
  'refinements', 'run_files', 'run_log', 'sessions', 'signal_reads', 'signals',
  'task_claims', 'task_dependencies', 'task_events', 'task_paths', 'task_runs', 'awareness_tasks',
] as const;

describe('advanced Awareness database contract', () => {
  it('creates every advanced module entity with an exact, healthy Awareness schema', () => {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);

    const tables = db.prepare(
      "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    ).all().map((row) => (row as { name: string }).name)
      .filter((name) => !name.startsWith('memories_fts'));
    const indexes = new Set(db.prepare(
      "SELECT name FROM sqlite_schema WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
    ).all().map((row) => (row as { name: string }).name));

    expect(tables).toEqual([...ADVANCED_TABLES].sort());
    expect(db.prepare('PRAGMA application_id').get()).toEqual({ application_id: AWARENESS_APPLICATION_ID });
    expect(db.prepare('PRAGMA integrity_check').get()).toEqual({ integrity_check: 'ok' });
    expect(db.prepare('PRAGMA foreign_key_check').all()).toEqual([]);
    expect([...indexes]).toEqual(expect.arrayContaining([
      'idx_awareness_memories_scope', 'idx_awareness_plans_scope', 'idx_awareness_tasks_plan_status',
      'idx_run_files_path_active', 'idx_awareness_locks_file_path', 'idx_signals_scope',
      'idx_awareness_agents_scope', 'idx_edit_log_scope', 'idx_harness_log_scope',
    ]));
    db.close();
  });

  it('rejects a canonical database with foreign-key damage when reopened', () => {
    const db = new DatabaseSync(':memory:');
    initDb(db);
    db.exec('PRAGMA foreign_keys = OFF');
    db.prepare(`
      INSERT INTO run_files(
        run_id, file_path, source, started_at, heartbeat_at, expires_at
      ) VALUES (?, ?, 'HOOK', ?, ?, ?)
    `).run('missing-run', '/tmp/orphan.ts', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', '2026-01-01T00:01:00.000Z');

    expect(() => initDb(db)).toThrow('canonical foreign_key_check failed with 1 row(s)');
    db.close();
  });
});
