import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';
import { releaseFileLock } from '../../src/intents-release.js';

let root: string;
let secondRoot: string;
let dbPath: string;
let first: AwarenessStore;
let second: AwarenessStore;

beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), 'aw-state-'));
  secondRoot = await mkdtemp(join(tmpdir(), 'aw-state-other-'));
  dbPath = join(root, 'awareness.sqlite3');
  first = openAwarenessStore({ workspace: root, dbPath });
  second = openAwarenessStore({ workspace: secondRoot, dbPath });
});

afterEach(async () => {
  first.close();
  second.close();
  await rm(root, { recursive: true, force: true });
  await rm(secondRoot, { recursive: true, force: true });
});

function database(): DatabaseSync {
  return new DatabaseSync(dbPath);
}

describe('host state contracts', () => {
  it('times out on a held lock, projects expiry without renewal, and prunes only its workspace', () => {
    const firstLock = first.acquireLock({
      filePath: 'src/held.ts', agentId: 'holder', reason: 'change host state', testPlan: 'state lock contract', ttlSeconds: 60,
    });
    const secondLock = second.acquireLock({
      filePath: 'src/held.ts', agentId: 'other-holder', reason: 'change other state', testPlan: 'other state lock contract', ttlSeconds: 60,
    });

    expect(first.waitForLock({ filePath: 'src/held.ts', agentId: 'waiter', waitMs: 30, retryIntervalMs: 25 }))
      .toMatchObject({ ok: false, lockFree: false, conflict: { runId: firstLock.runId, agentId: 'holder' } });

    const db = database();
    try {
      const expired = new Date(Date.now() - 60_000).toISOString();
      db.prepare('UPDATE awareness_locks SET expires_at = ? WHERE run_id IN (?, ?)')
        .run(expired, firstLock.runId, secondLock.runId);

      expect(first.waitForLock({ filePath: 'src/held.ts', agentId: 'waiter', waitMs: 0 }))
        .toMatchObject({ ok: true, lockFree: true, conflict: null });
      expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_locks WHERE run_id = ?').get(firstLock.runId))
        .toEqual({ count: 1 });

      expect(first.pruneLocks()).toEqual({ dryRun: true, matched: 1, deleted: 0 });
      expect(first.pruneLocks({ dryRun: false })).toEqual({ dryRun: false, matched: 1, deleted: 1 });
      expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_locks WHERE run_id = ?').get(firstLock.runId))
        .toEqual({ count: 0 });
      expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_locks WHERE run_id = ?').get(secondLock.runId))
        .toEqual({ count: 1 });
    } finally {
      db.close();
    }
  });

  it('fences release and end mutations by run ownership while preserving partial work', () => {
    const lock = first.acquireLock({
      filePath: 'src/locked.ts', agentId: 'owner', reason: 'edit locked file', testPlan: 'lock ownership contract', ttlSeconds: 60,
    });
    first.startWork({
      filePath: 'src/partial.ts', runId: lock.runId, agentId: 'owner', reason: 'edit related file', testPlan: 'lock ownership contract', ttlSeconds: 60,
    });

    expect(() => first.releaseLock({ filePath: 'src/locked.ts', runId: 'run_missing', agentId: 'owner' }))
      .toThrow(/run not found/);
    expect(() => first.releaseLock({ filePath: 'src/locked.ts', runId: lock.runId, agentId: 'intruder' }))
      .toThrow(/ownership mismatch/);
    expect(() => first.endWork({ filePath: 'src/partial.ts', runId: lock.runId, agentId: 'intruder' }))
      .toThrow(/ownership mismatch/);

    expect(first.endWork({ filePath: 'src/partial.ts', runId: lock.runId, agentId: 'owner' }))
      .toEqual({ ended: true });
    expect(first.endWork({ filePath: 'src/partial.ts', runId: lock.runId, agentId: 'owner' }))
      .toEqual({ ended: false });
    expect(first.showWork({ filePath: 'src/locked.ts' }))
      .toMatchObject([{ runId: lock.runId, agentId: 'owner' }]);
    expect(first.listWork()).toMatchObject([{ runId: lock.runId, filePath: expect.stringMatching(/src\/locked\.ts$/) }]);
    expect(first.releaseLock({ filePath: 'src/locked.ts', runId: lock.runId, agentId: 'owner' }))
      .toEqual({ released: true });
  });

  it('scopes handoffs and keeps cleared records available only when requested', () => {
    const handoff = first.addHandoff({
      agentId: 'owner', summary: 'parser edit is ready for review', files: ['src/parser.ts', 'tests/parser.test.ts'],
    });
    expect(first.listHandoffs()).toMatchObject([{ handoffId: handoff.handoffId, agentId: 'owner' }]);
    expect(second.listHandoffs()).toEqual([]);
    expect(first.clearHandoff({ handoffId: handoff.handoffId })).toEqual({ cleared: true });
    expect(first.clearHandoff({ handoffId: handoff.handoffId })).toEqual({ cleared: false });
    expect(first.listHandoffs()).toEqual([]);
    expect(first.listHandoffs({ includeCleared: true })).toMatchObject([{ handoffId: handoff.handoffId }]);
  });

  it('audits mixed task and work verification debt, filters task plans, and fences stale verification', () => {
    const plan = first.createPlan({ agentId: 'lead', title: 'Audit plan', goal: 'Exercise task audit filtering.' });
    const task = first.addTask({
      planId: plan.planId, title: 'Audited task', paths: ['src/task.ts'], agentId: 'lead',
      reasoning: 'Need a task-backed verification receipt.', acceptance: 'A receipt settles this task.',
    });
    const claim = first.claimTask({ taskId: task.taskId, agentId: 'task-owner' });
    first.doneTask({ taskId: task.taskId, runId: claim.runId!, agentId: 'task-owner' });
    const work = first.acquireLock({
      filePath: 'src/work.ts', agentId: 'work-owner', reason: 'Need standalone verification debt', testPlan: 'work audit contract', ttlSeconds: 60,
    });

    const db = database();
    try {
      expect(releaseFileLock(db, { agentId: 'work-owner', runId: work.runId, status: 'PENDING' }))
        .toMatchObject({ released: true, status: 'PENDING' });
      const taskRun = db.prepare('SELECT updated_at FROM task_runs WHERE run_id = ?').get(claim.runId) as { updated_at: string };

      expect(first.auditChecks({ minAgeMs: 0 }).pending.map((row) => row.runId).sort())
        .toEqual([claim.runId, work.runId].sort());
      expect(first.auditChecks({ planId: plan.planId, minAgeMs: 0 }).pending.map((row) => row.runId))
        .toEqual([claim.runId]);
      expect(first.auditChecks({ agentId: 'work-owner', minAgeMs: 0 }).pending.map((row) => row.runId))
        .toEqual([work.runId]);
      expect(() => first.markCheck({
        taskId: task.taskId, runId: claim.runId!, doneAt: '1970-01-01T00:00:00.000Z',
        agentId: 'task-owner', message: 'stale receipt',
      })).toThrow(/stale task completion/);

      expect(first.markCheck({
        taskId: task.taskId, runId: claim.runId!, doneAt: taskRun.updated_at,
        agentId: 'task-owner', message: 'task verification passed',
      })).toMatchObject({ status: 'DONE' });
      expect(first.auditChecks({ planId: plan.planId, minAgeMs: 0 })).toMatchObject({ ok: true, pendingCount: 0 });
      expect(first.auditChecks({ minAgeMs: 0 }).pending.map((row) => row.runId)).toEqual([work.runId]);
    } finally {
      db.close();
    }
  });
});
