import { afterEach, describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { attendAwareness } from '../src/attend-query.js';

const cleanups: Array<() => void> = [];
afterEach(() => cleanups.splice(0).forEach(cleanup => cleanup()));

function fixture() {
  const workspace = mkdtempSync(join(realpathSync(tmpdir()), 'attend-owner-'));
  const db = new DatabaseSync(':memory:');
  initDb(db);
  cleanups.push(() => { db.close(); rmSync(workspace, { recursive: true, force: true }); });
  const insertRun = db.prepare(`INSERT INTO task_runs
    (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
    VALUES (?, 'WORK', ?, 'bounded work', 'observed check', 'PENDING', ?, ?, ?)`);
  const run = (id: string, owner: string, date: string) => insertRun.run(id, owner, workspace, date, date);
  return { db, workspace, run };
}

describe('attend ownership before presentation limits', () => {
  it('finds an older owned run beyond the global 500-row candidate cap', () => {
    const { db, workspace, run } = fixture();
    run('run_owned', 'owner', '2025-01-01T00:00:00Z');
    for (let i = 0; i < 510; i++) run(`run_peer_${i}`, 'peer', '2026-01-01T00:00:00Z');
    const result = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', limit: 1, compact: true });
    expect(result.workboard.Verify?.[0]?.id).toBe('run_owned');
    expect(result.counts?.Verify).toBe(511);
    expect(result.next.target?.run_id).toBe('run_owned');
  });

  it('prioritizes an owned standalone run over foreign task verification', () => {
    const { db, workspace, run } = fixture();
    const now = '2026-01-01T00:00:00Z';
    db.prepare(`INSERT INTO awareness_plans
      (plan_id, name, objective, lead_agent_id, status, workspace_path, doc_dir, created_at, updated_at)
      VALUES ('plan_test', 'test', 'test', 'peer', 'ACTIVE', ?, '.', ?, ?)`).run(workspace, now, now);
    db.prepare(`INSERT INTO awareness_tasks
      (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
      VALUES ('task_peer', 'plan_test', 'peer verification', 'test', 'check', 'VERIFY', 1, 'peer', ?, ?)`).run(now, now);
    run('run_owned', 'owner', now);
    const result = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', limit: 1, compact: true });
    expect(result.workboard.Verify?.[0]?.id).toBe('run_owned');
    expect(result.counts?.Verify).toBe(2);
    expect(result.next.action).toBe('verify_owned_work');
  });

  it('selects a pending task by its run owner before the task cap, even after its claim ends', () => {
    const { db, workspace, run } = fixture();
    const now = '2026-01-01T00:00:00Z';
    db.prepare(`INSERT INTO awareness_plans
      (plan_id, name, objective, lead_agent_id, status, workspace_path, doc_dir, created_at, updated_at)
      VALUES ('plan_test', 'test', 'test', 'lead', 'ACTIVE', ?, '.', ?, ?)`).run(workspace, now, now);
    const task = db.prepare(`INSERT INTO awareness_tasks
      (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
      VALUES (?, 'plan_test', 'verify task', 'test', 'check', 'VERIFY', ?, 'lead', ?, ?)`);
    for (let i = 0; i < 510; i++) task.run(`task_peer_${i}`, 9, now, now);
    task.run('task_owned', 1, now, now);
    run('run_owned', 'owner', now);
    db.prepare("UPDATE task_runs SET task_id = 'task_owned' WHERE run_id = 'run_owned'").run();
    const result = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', limit: 1, compact: true });
    expect(result.workboard.Verify?.[0]?.id).toBe('task_owned');
    expect(result.workboard.Verify?.[0]?.agent_id).toBe('owner');
    expect(result.counts?.Verify).toBe(511);
    expect(result.next.target?.run_id).toBe('run_owned');
  });

  it('preserves a requested peer lock before both file candidate and presentation limits', () => {
    const { db, workspace, run } = fixture();
    const now = new Date().toISOString();
    const future = new Date(Date.now() + 60_000).toISOString();
    run('run_peer', 'peer', now);
    db.prepare("UPDATE task_runs SET status = 'ACTIVE' WHERE run_id = 'run_peer'").run();
    const file = db.prepare(`INSERT INTO run_files
      (run_id, file_path, source, started_at, heartbeat_at, expires_at)
      VALUES ('run_peer', ?, 'EXPLICIT', ?, ?, ?)`);
    for (let i = 0; i < 810; i++) file.run(join(workspace, `a${i}.ts`), now, now, future);
    const target = join(workspace, 'z.ts');
    file.run(target, now, now, future);
    db.prepare(`INSERT INTO awareness_locks (lock_id, file_path, run_id, acquired_at, expires_at)
      VALUES ('lock_peer', ?, 'run_peer', ?, ?)`).run(target, now, future);
    const result = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', file: 'z.ts', limit: 1, compact: true });
    expect(result.operational_state.coordination.locks_observed).toBe(1);
    expect(result.regulation.actions).toContain('inspect_lock');
    expect(result.next).toMatchObject({ action: 'inspect_lock', target: { file: 'z.ts' } });
  });

  it('does not route an agent back to inspect its own advisory file presence', () => {
    const { db, workspace, run } = fixture();
    const now = new Date().toISOString();
    run('run_owned', 'owner', now);
    db.prepare("UPDATE task_runs SET status = 'ACTIVE' WHERE run_id = 'run_owned'").run();
    db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
      VALUES ('run_owned', ?, 'EXPLICIT', ?, ?, ?)`).run(join(workspace, 'own.ts'), now, now, new Date(Date.now() + 60_000).toISOString());
    const result = attendAwareness(db, { workspacePath: workspace, agentId: 'owner', compact: true });
    expect(result.regulation.actions).toEqual([]);
    expect(result.next.action).toBe('continue');
    expect(result.next).not.toHaveProperty('command');
  });
});
