import { DatabaseSync } from 'node:sqlite';
import { chmodSync, mkdtempSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../src/coordination/open.js';
import { AWARENESS_APPLICATION_ID } from '../src/storage-scope.js';

const AWARENESS_TABLES = [
   'authorization_receipts',
  'awareness_agents', 'awareness_locks', 'awareness_memories', 'awareness_plans', 'awareness_tasks', 'capability_receipts',
  'delivery_state', 'edit_log', 'event_acknowledgements', 'event_consumers',
  'event_outbox', 'handoffs', 'harness_log', 'hook_receipts',
   'memory_refs',
  'pending_interactions', 'plan_docs', 'plan_members',  'refinements',
  'run_files', 'run_log', 'sessions', 'signal_reads', 'signals',
  'task_claims', 'task_dependencies', 'task_events', 'task_paths', 'task_runs',

] as const;

describe('Awareness database contract', () => {
  it('creates every Awareness entity without Agent control relations', () => {
    const dir = mkdtempSync(join(tmpdir(), 'awareness-shared-contract-'));
    chmodSync(dir, 0o755);
    const dbPath = join(dir, 'awareness.sqlite3');
    const store = openAwarenessStore({ workspace: dir, dbPath });
    store.close();
    expect(statSync(dir).mode & 0o777).toBe(0o700);
    expect(statSync(dbPath).mode & 0o777).toBe(0o600);
    const db = new DatabaseSync(dbPath);
    try {
      const tables = db.prepare(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
      ).all().map((row) => (row as { name: string }).name)
        .filter((name) => !name.startsWith('memories_fts'));
      const indexes = new Set(db.prepare(
        "SELECT name FROM sqlite_schema WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
      ).all().map((row) => (row as { name: string }).name));

      expect(tables).toEqual([...AWARENESS_TABLES].sort());
      expect(db.prepare('PRAGMA application_id').get())
        .toEqual({ application_id: AWARENESS_APPLICATION_ID });
      expect(db.prepare('PRAGMA integrity_check').get()).toEqual({ integrity_check: 'ok' });
      expect(db.prepare('PRAGMA foreign_key_check').all()).toEqual([]);
      expect([...indexes]).toEqual(expect.arrayContaining([
        'idx_awareness_plans_scope', 'idx_awareness_tasks_plan_status', 'idx_run_files_path_active',
        'idx_signals_to_agent', 'idx_event_outbox_workspace_sequence',
        'idx_interactions_session_status', 'idx_authorization_plan_revision',
      ]));
    } finally {
      db.close();
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
