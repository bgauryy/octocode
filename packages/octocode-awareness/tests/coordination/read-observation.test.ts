import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';
import { getWorkspaceStatus } from '../../src/maintenance-workspace.js';

let workspace: string;
let aw: AwarenessStore;

beforeEach(async () => {
  workspace = await mkdtemp(join(tmpdir(), 'aw-read-'));
  aw = openAwarenessStore({ workspace });
});

afterEach(async () => {
  aw.close();
  await rm(workspace, { recursive: true, force: true });
});

it('projects expired state without mutating stored rows', () => {
  const plan = aw.createPlan({ agentId: 'lead', title: 'Read-only status', goal: 'Project expired state without mutation.' });
  const task = aw.addTask({ planId: plan.planId, title: 'leased', paths: ['expired.ts'], agentId: 'lead', reasoning: 'Exercise projected expiry.', acceptance: 'Stored rows remain durable.' });
  const claim = aw.claimTask({ taskId: task.taskId, agentId: 'agent-a' });
  aw.acquireLock({ filePath: 'expired.ts', runId: claim.runId!, agentId: 'agent-a', reason: 'observe expiry' });
  aw.startWork({ filePath: 'expired.ts', runId: claim.runId!, agentId: 'agent-a', reason: 'observe expiry', testPlan: 'focused expiry test' });

  const db = new DatabaseSync(aw.dbPath);
  const expired = new Date(Date.now() - 60_000).toISOString();
  db.prepare('UPDATE task_claims SET expires_at = ? WHERE task_id = ?').run(expired, task.taskId);
  db.prepare('UPDATE awareness_locks SET expires_at = ? WHERE run_id = ?').run(expired, claim.runId);
  db.prepare('UPDATE run_files SET expires_at = ? WHERE run_id = ?').run(expired, claim.runId);

  const pausedPlan = aw.createPlan({ agentId: 'lead', title: 'Paused work', goal: 'Exclude paused work from ready counts.' });
  aw.addTask({ planId: pausedPlan.planId, title: 'paused task', paths: ['paused.ts'], agentId: 'lead', reasoning: 'Exercise plan status filtering.', acceptance: 'Paused plans do not present ready work.' });
  db.prepare("UPDATE awareness_plans SET status = 'PAUSED' WHERE plan_id = ?").run(pausedPlan.planId);
  const claimBefore = db.prepare('SELECT expires_at FROM task_claims WHERE task_id = ?').get(task.taskId);
  const pausedBefore = db.prepare('SELECT status FROM awareness_plans WHERE plan_id = ?').get(pausedPlan.planId);

  expect(aw.status()).toMatchObject({ inProgressTasks: 0, readyTasks: 1, locks: 0, work: 0 });
  expect(getWorkspaceStatus(db, { workspace_path: workspace })).toMatchObject({ ready_tasks: 1 });
  expect(aw.listTasks({ planId: plan.planId })).toMatchObject([{ status: 'OPEN', agentId: null }]);
  expect(aw.listLocks()).toEqual([]);
  expect(aw.listWork()).toEqual([]);

  expect(db.prepare('SELECT status FROM awareness_tasks WHERE task_id = ?').get(task.taskId)).toEqual({ status: 'IN_PROGRESS' });
  expect((db.prepare('SELECT COUNT(*) AS count FROM awareness_locks WHERE run_id = ?').get(claim.runId) as { count: number }).count).toBe(1);
  expect((db.prepare('SELECT COUNT(*) AS count FROM run_files WHERE run_id = ?').get(claim.runId) as { count: number }).count).toBe(1);
  expect(db.prepare('SELECT expires_at FROM task_claims WHERE task_id = ?').get(task.taskId)).toEqual(claimBefore);
  expect(db.prepare('SELECT status FROM awareness_plans WHERE plan_id = ?').get(pausedPlan.planId)).toEqual(pausedBefore);
  db.close();
});
