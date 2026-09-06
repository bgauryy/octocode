import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';

let workspace: string;
let aw: AwarenessStore;
const step = (sourceStepKey: string, title: string, dependsOnStepKeys?: string[]) => ({
  sourceStepKey, title, paths: [`src/${sourceStepKey}.ts`], reasoning: `${title} is required.`,
  acceptance: `${title} is verified.`, dependsOnStepKeys,
});
const input = (steps = [step('one', 'One'), step('two', 'Two', ['one'])]) => ({
  sourcePlanKey: 'rfc-ship', sourceKind: 'rfc', title: 'Ship RFC', goal: 'Ship through canonical runs.',
  rfcPath: 'RFC.md', rfcRevision: 'sha256:one', agentId: 'lead', steps,
});
beforeEach(async () => { workspace = await mkdtemp(join(tmpdir(), 'aw-plan-')); aw = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') }); });
afterEach(async () => { aw?.close(); await rm(workspace, { recursive: true, force: true }); });

describe('canonical source plan graph', () => {
  it('is idempotent and persists source identity with canonical dependencies', () => {
    const first = aw.materializePlanGraph(input());
    const second = aw.materializePlanGraph(input());
    expect(second.plan.planId).toBe(first.plan.planId);
    expect(second.tasks.get('one')!.taskId).toBe(first.tasks.get('one')!.taskId);
    expect(first.plan).toMatchObject({ status: 'ACTIVE', sourceKind: 'rfc', sourceKey: 'rfc-ship', rfcRevision: 'sha256:one' });
    expect(first.tasks.get('two')!.dependencies).toEqual([first.tasks.get('one')!.taskId]);
  });

  it('rejects revision conflicts and malformed graphs before persistence', () => {
    aw.materializePlanGraph(input());
    expect(() => aw.materializePlanGraph({ ...input(), rfcRevision: 'sha256:two' })).toThrow(/revision conflict/);
    expect(() => aw.materializePlanGraph(input([step('same', 'One'), step('same', 'Two')]))).toThrow(/duplicate source step key/);
    expect(() => aw.materializePlanGraph(input([step('a', 'A', ['missing'])]))).toThrow(/dependency step key/);
  });

  it('does not rewrite task contracts after a task starts', () => {
    const graph = aw.materializePlanGraph(input());
    const claimed = aw.claimTask({ taskId: graph.tasks.get('one')!.taskId, agentId: 'worker' });
    expect(() => aw.materializePlanGraph(input([step('one', 'Changed'), step('two', 'Two', ['one'])]))).toThrow(/IN_PROGRESS task contract/);
    expect(claimed.runId).toMatch(/^run_/);
  });

  it('cancels only quiescent canonical plans', () => {
    const graph = aw.materializePlanGraph(input());
    expect(aw.abandonPlan({ planId: graph.plan.planId, agentId: 'lead' })).toMatchObject({ plan: { status: 'CANCELLED' }, cancelled: 2 });
    expect(aw.listReadyTasks({ planId: graph.plan.planId })).toEqual([]);
  });
});
