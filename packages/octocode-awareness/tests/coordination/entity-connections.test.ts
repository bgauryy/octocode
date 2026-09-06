import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';

let workspace: string;
let aw: AwarenessStore;
function plan(title: string) { return aw.createPlan({ title, goal: `${title} objective`, agentId: 'lead' }); }
function task(planId: string, title: string) { return aw.addTask({ planId, title, paths: [`src/${title}.ts`], reasoning: `${title} rationale`, acceptance: `${title} acceptance`, agentId: 'lead' }); }
beforeEach(() => { workspace = mkdtempSync(join(tmpdir(), 'aw-entity-')); aw = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') }); });
afterEach(() => { aw.close(); rmSync(workspace, { recursive: true, force: true }); });

describe('canonical coordination entity connections', () => {
  it('rejects missing, cross-plan, and cyclic dependency targets', () => {
    const firstPlan = plan('first');
    const a = task(firstPlan.planId, 'a');
    const b = task(firstPlan.planId, 'b');
    expect(() => aw.addTaskDependency({ taskId: a.taskId, dependsOnTaskId: 'task_missing', agentId: 'lead' })).toThrow(/task not found|both dependency tasks/i);
    const other = task(plan('other').planId, 'other');
    expect(() => aw.addTaskDependency({ taskId: a.taskId, dependsOnTaskId: other.taskId, agentId: 'lead' })).toThrow(/within one plan/i);
    aw.addTaskDependency({ taskId: b.taskId, dependsOnTaskId: a.taskId, agentId: 'lead' });
    expect(() => aw.addTaskDependency({ taskId: a.taskId, dependsOnTaskId: b.taskId, agentId: 'lead' })).toThrow(/cycle/i);
  });

  it('permits takeover after a claim expires but no read settles it', () => {
    const work = task(plan('leases').planId, 'lease');
    const old = aw.claimTask({ taskId: work.taskId, agentId: 'old', leaseSeconds: 1 });
    expect(old.runId).toMatch(/^run_/);
    const db = new DatabaseSync(aw.dbPath);
    try { db.prepare("UPDATE task_claims SET expires_at = '2000-01-01T00:00:00Z' WHERE task_id = ?").run(work.taskId); } finally { db.close(); }
    expect(aw.getTask(work.taskId)).toMatchObject({ status: 'OPEN' });
    expect(aw.claimTask({ taskId: work.taskId, agentId: 'new' })).toMatchObject({ status: 'IN_PROGRESS', agentId: 'new' });
  });

  it('refuses cyclic source projections atomically', () => {
    expect(() => aw.materializePlanGraph({
      sourceKind: 'rfc', sourcePlanKey: 'cycle', title: 'Cycle', goal: 'No cycles.', agentId: 'lead',
      steps: [
        { sourceStepKey: 'a', title: 'A', paths: ['src/a.ts'], reasoning: 'A rationale', acceptance: 'A acceptance', dependsOnStepKeys: ['b'] },
        { sourceStepKey: 'b', title: 'B', paths: ['src/b.ts'], reasoning: 'B rationale', acceptance: 'B acceptance', dependsOnStepKeys: ['a'] },
      ],
    })).toThrow(/cycle/);
    expect(aw.listPlans()).toEqual([]);
  });
});
