import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';

let workspace: string;
let aw: AwarenessStore;
function task(title: string, dependsOn?: string[]) {
  const plan = aw.listPlans()[0]!;
  return aw.addTask({ planId: plan.planId, title, paths: [`src/${title}.ts`], reasoning: `${title} rationale`, acceptance: `${title} acceptance`, agentId: 'lead', dependsOn });
}
beforeEach(() => { workspace = mkdtempSync(join(tmpdir(), 'aw-transition-')); aw = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') }); aw.createPlan({ title: 'Transitions', goal: 'Prove canonical transitions.', agentId: 'lead' }); });
afterEach(() => { aw.close(); rmSync(workspace, { recursive: true, force: true }); });

describe('canonical task transitions', () => {
  it('requires a matching active run to submit and a matching pending run to verify', () => {
    const work = task('work');
    const claim = aw.claimTask({ taskId: work.taskId, agentId: 'worker' });
    expect(() => aw.doneTask({ taskId: work.taskId, runId: 'run_wrong', agentId: 'worker' })).toThrow(/active claimant/);
    const submitted = aw.doneTask({ taskId: work.taskId, runId: claim.runId!, agentId: 'worker' });
    expect(() => aw.markCheck({ taskId: work.taskId, runId: 'run_wrong', doneAt: submitted.updatedAt, agentId: 'worker', message: 'bad' })).toThrow(/run not found|task\/run/i);
    expect(aw.markCheck({ taskId: work.taskId, runId: claim.runId!, doneAt: submitted.updatedAt, agentId: 'worker', message: 'passed' }))
      .toMatchObject({ status: 'DONE', verifiedAt: expect.any(String) });
  });

  it('requires completed dependencies before ready work can be claimed', () => {
    const first = task('first');
    const second = task('second', [first.taskId]);
    expect(aw.listReadyTasks().map((row) => row.taskId)).toEqual([first.taskId]);
    const claim = aw.claimTask({ taskId: first.taskId, agentId: 'worker' });
    const submitted = aw.doneTask({ taskId: first.taskId, runId: claim.runId!, agentId: 'worker' });
    aw.markCheck({ taskId: first.taskId, runId: claim.runId!, doneAt: submitted.updatedAt, agentId: 'worker', message: 'passed' });
    expect(aw.listReadyTasks().map((row) => row.taskId)).toEqual([second.taskId]);
  });

  it('keeps failed verification terminal until the canonical lead retry operation', () => {
    const work = task('failure');
    const claim = aw.claimTask({ taskId: work.taskId, agentId: 'worker' });
    const submitted = aw.doneTask({ taskId: work.taskId, runId: claim.runId!, agentId: 'worker' });
    expect(aw.markCheck({ taskId: work.taskId, runId: claim.runId!, doneAt: submitted.updatedAt, agentId: 'worker', message: 'failed', status: 'FAILED' }))
      .toMatchObject({ status: 'FAILED' });
    expect(() => aw.claimTask({ taskId: work.taskId, agentId: 'worker' })).toThrow(/not ready/i);
  });
});
