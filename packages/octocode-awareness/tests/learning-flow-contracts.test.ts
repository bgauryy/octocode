import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { attendAwareness } from '../src/attend-query.js';
import { inspectMaintenancePressure } from '../src/maintenance-pressure.js';
import { getMemory } from '../src/memory-recall.js';
import { insertMemory } from '../src/memory-write.js';
import { agentSignal } from '../src/notifications-signals.js';
import { formatAwarenessQueryResult, queryAwareness } from '../src/repo-query.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('read, act, and learn flow contracts', () => {
  it('does not call a missing file reference verified and smart recall really broadens filters', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'awareness-learning-trust-'));
    try {
      const db = freshDb();
      const missing = join(dir, 'missing.ts');
      const { memoryId } = (await insertMemory(db, {
        agentId: 'learning-agent',
        taskContext: 'cache source evidence',
        observation: 'Validate current source before applying recalled cache rules.',
        importance: 8,
        label: 'GOTCHA',
        references: [`file:${missing}`],
        workspacePath: dir,
      }));

      const packet = attendAwareness(db, {
        agentId: 'learning-agent', workspacePath: dir, query: 'cache source evidence', compact: true,
      });
      expect(packet.evidence[0]?.id).toBe(memoryId);
      expect(packet.evidence[0]?.trust).not.toBe('verified_lead');

      const broadened = (await getMemory(db, {
        query: 'cache source evidence', workspacePath: dir, label: ['SECURITY'], smart: true,
      }));
      expect(broadened.memories.map(memory => memory.memory_id)).toContain(memoryId);
      expect((broadened as typeof broadened & { smart_expanded?: boolean }).smart_expanded).toBe(true);
      expect((broadened as typeof broadened & { smart_dropped_filters?: string[] }).smart_dropped_filters)
        .toContain('label');
      expect(broadened.judgment_reason ?? '').not.toContain('retry with --smart');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('keeps foreign maintenance pressure informational instead of hijacking next work', () => {
    const db = freshDb();
    db.prepare(`INSERT INTO task_runs
      (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES ('run_foreign_old', 'WORK', 'other-agent', 'old peer debt', 'peer check', 'PENDING', '/repo', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')`).run();

    const packet = attendAwareness(db, {
      agentId: 'worker', workspacePath: '/repo', query: 'implement current feature', compact: true,
    });
    expect(packet.counts?.Maintenance).toBeGreaterThan(0);
    expect(packet.next.operation).toBeUndefined();
    expect(packet.next.action).toBe('continue');
  });

  it('marks bounded explicit exports partial and escapes CSV formula injection', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'awareness-share-safe-'));
    try {
      const db = freshDb();
      for (let index = 0; index < 55; index += 1) {
        (await insertMemory(db, {
          agentId: 'learning-agent',
          taskContext: `lesson ${index}`,
          observation: index === 0 ? '=HYPERLINK("https://example.invalid","click")' : `Distinct lesson ${index}`,
          importance: 5,
          label: 'WORKFLOW',
          workspacePath: dir,
          preComputedSimilar: [],
        }));
      }
      agentSignal(db, {
        action: 'publish',
        agentId: 'learning-agent',
        workspacePath: dir,
        kind: 'fyi',
        subject: 'share probe',
        body: `PRIVATE_PLACEHOLDER ${join(dir, 'secret.ts')}`,
        files: [join(dir, 'secret.ts')],
      });

      const bounded = queryAwareness(db, { view: 'memories', workspacePath: dir, limit: 10 });
      expect(bounded.count).toBe(10);
      expect(bounded.is_partial).toBe(true);
      expect(bounded.continuation).toBeTruthy();
      const boundedCsv = formatAwarenessQueryResult(bounded, 'csv');
      expect(boundedCsv).toContain('__awareness_is_partial');
      expect(boundedCsv).toContain('true');
      const explicitCsv = formatAwarenessQueryResult(
        queryAwareness(db, { view: 'memories', workspacePath: dir, limit: 500 }),
        'csv',
      );

      expect(explicitCsv).toContain("'=HYPERLINK");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 30_000);

  it('reports only age-qualified maintenance pressure and emits selector-bearing actions', async () => {
    const db = freshDb();
    const workspace = '/repo';
    const old = '2020-01-01T00:00:00Z';
    const now = new Date().toISOString();
    db.prepare(`
      INSERT INTO task_runs
        (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES
        ('run_old', 'WORK', 'owner', 'old pending', 'test', 'PENDING', ?, ?, ?),
        ('run_fresh', 'WORK', 'owner', 'fresh pending', 'test', 'PENDING', ?, ?, ?)
    `).run(workspace, old, old, workspace, now, now);
    agentSignal(db, {
      action: 'publish', agentId: 'owner', workspacePath: workspace,
      kind: 'fyi', subject: 'old signal', body: 'review me',
    });
    db.prepare("UPDATE signals SET created_at = ? WHERE subject = 'old signal'").run(old);
    const memory = (await insertMemory(db, {
      agentId: 'owner', taskContext: 'stale ref', observation: 'review old path', importance: 5,
      references: ['file:/repo/missing.ts'], workspacePath: workspace,
    }));
    db.prepare('UPDATE awareness_memories SET created_at = ?, updated_at = ? WHERE memory_id = ?')
      .run(old, old, memory.memoryId);

    const preview = inspectMaintenancePressure(db, { workspace_path: workspace });
    expect(preview.pressure_age_days).toBe(1);
    expect(preview.stale_pending_runs).toBe(1);
    expect(preview.stale_active_runs).toBe(0);
    expect(preview.stale_open_signals).toBe(1);
    expect(preview.stale_handoff_signals).toBe(0);
    expect(preview.stale_missing_refs).toBe(1);
    expect(db.prepare("SELECT status FROM task_runs WHERE run_id = 'run_old'").get())
      .toEqual({ status: 'PENDING' });

    const board = queryAwareness(db, { view: 'workboard', workspacePath: workspace, limit: 10 });
    const maintenance = board.rows.filter(row => row.column === 'Maintenance');
    expect(maintenance.length).toBeGreaterThan(0);
    expect(maintenance.find(row => row.id === 'stale-open-signals')?.action).toBe('message list --all --limit 5 --compact');
    expect(maintenance.find(row => row.id === 'stale-missing-memory-refs')?.raw_ids).toEqual([memory.memoryId]);
  });

});
