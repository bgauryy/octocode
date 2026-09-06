import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';

let workspace: string;
let store: AwarenessStore;

beforeEach(async () => {
  workspace = await mkdtemp(join(tmpdir(), 'aw-canonical-graph-'));
  store = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') });
});

afterEach(async () => {
  store?.close();
  await rm(workspace, { recursive: true, force: true });
});

describe('canonical projected plans', () => {
  it('materializes source identity, tasks, and dependencies in canonical tables', () => {
    const input = {
      sourceKind: 'external-agent',
      sourcePlanKey: 'host-plan-42',
      title: 'Canonical external plan',
      goal: 'Finish the host plan through durable task runs.',
      agentId: 'host-agent',
      steps: [
        {
          sourceStepKey: 'design', title: 'Design contract', paths: ['src/design.ts'],
          reasoning: 'The task contract must be explicit.', acceptance: 'Contract is recorded.',
          checkCommand: 'yarn test design',
        },
        {
          sourceStepKey: 'implement', title: 'Implement contract', paths: ['src/implement.ts'],
          reasoning: 'Implementation follows the durable contract.', acceptance: 'Implementation is covered.',
          dependsOnStepKeys: ['design'],
        },
      ],
    };
    const first = store.materializePlanGraph(input);
    const second = store.materializePlanGraph(input);

    expect(second.plan.planId).toBe(first.plan.planId);
    expect(second.tasks.get('design')!.taskId).toBe(first.tasks.get('design')!.taskId);
    expect(first.plan).toMatchObject({ status: 'ACTIVE', sourceKind: 'external-agent', sourceKey: 'host-plan-42' });
    expect(first.tasks.get('design')).toMatchObject({ status: 'OPEN', checkCommand: 'yarn test design', runId: null });
    expect(first.tasks.get('implement')!.dependencies).toEqual([first.tasks.get('design')!.taskId]);

    const claimed = store.claimTask({ taskId: first.tasks.get('design')!.taskId, agentId: 'worker' });
    expect(claimed).toMatchObject({ status: 'IN_PROGRESS', agentId: 'worker', runId: expect.stringMatching(/^run_/) });
    expect(store.listReadyTasks({ planId: first.plan.planId })).toEqual([]);
  });

  it('rejects underspecified host steps before it creates a plan document', () => {
    expect(() => store.materializePlanGraph({
      sourcePlanKey: 'incomplete-host-plan', sourceKind: 'external-agent', title: 'Incomplete',
      goal: 'This cannot materialize.', agentId: 'host-agent',
      steps: [{ sourceStepKey: 'missing-contract', title: 'Missing fields', paths: ['src/missing.ts'] }],
    })).toThrow(/reasoning/i);
    expect(store.getPlanBySourceKey({ sourceKind: 'external-agent', sourceKey: 'incomplete-host-plan' })).toBeNull();
  });
});
