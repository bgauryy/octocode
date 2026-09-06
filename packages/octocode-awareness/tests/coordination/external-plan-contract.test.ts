import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, expect, it } from 'vitest';
import { completeExternalPlanTask, projectExternalPlan } from '../../src/coordination/external-plan.js';
import { openAwarenessStore } from '../../src/coordination/open.js';

let workspace: string;
beforeEach(async () => { workspace = await mkdtemp(join(tmpdir(), 'aw-projection-contract-')); });
afterEach(async () => { await rm(workspace, { recursive: true, force: true }); });

function input(key = 'host-plan') {
  return {
    requestedScope: 'shared' as const, workspace, sourcePlanKey: key,
    title: 'Host projection', goal: 'Keep host claims and verification in one ledger.', agentId: 'owner',
    steps: [{ id: 'step', text: 'Implement adapter', status: 'doing' as const, paths: ['src/adapter.ts'],
      reasoning: 'Hosts share one task identity.', acceptance: 'The adapter passes its check.', checkCommand: 'yarn test adapter' }],
  };
}

it('adopts a matching live claim without duplicating tasks and rejects a cross-plan mapping', () => {
  const original = projectExternalPlan(input());
  const taskId = original.taskIdsByStepId!.step!;
  expect(projectExternalPlan({ ...input('other-host'), requestedScope: 'auto' })).toMatchObject({
    adopted: true, awarenessPlanId: original.awarenessPlanId, taskIdsByStepId: { step: taskId },
  });
  expect(projectExternalPlan({ ...input('other-host'), awarenessPlanId: original.awarenessPlanId,
    steps: [{ ...input().steps[0]!, awarenessTaskId: taskId }] })).toMatchObject({ adopted: true });
  const store = openAwarenessStore({ workspace });
  try {
    const other = store.createPlan({ title: 'Other plan', goal: 'Separate ownership.', agentId: 'owner' });
    expect(() => projectExternalPlan({ ...input(), awarenessPlanId: other.planId,
      steps: [{ ...input().steps[0]!, awarenessTaskId: taskId }] })).toThrow(/another plan/);
    expect(store.listTasks({ planId: original.awarenessPlanId })).toHaveLength(1);
    const result = completeExternalPlanTask({ workspace, taskId, agentId: 'owner',
      receipt: { command: 'yarn test adapter', status: 'SUCCESS', message: 'adapter suite passed' } });
    expect(result).toMatchObject({ verified: true, task: {
      status: 'DONE', verifiedBy: 'owner', verificationMessage: 'adapter suite passed', verifiedAt: expect.any(String),
    } });
    expect(store.listTasks({ planId: original.awarenessPlanId })[0]).toEqual(result.task);
  } finally { store.close(); }
});

it('keeps unmatched automatic work local and rejects empty shared plans', () => {
  expect(projectExternalPlan({ ...input(), requestedScope: 'auto' })).toEqual({ scope: 'session', adopted: false });
  expect(() => projectExternalPlan({ ...input(), steps: [] })).toThrow(/at least one execution step/);
});

it('preserves the claim when receipts are missing or invalid and records a failed observed check', () => {
  const projected = projectExternalPlan(input());
  const taskId = projected.taskIdsByStepId!.step!;
  const completion = { workspace, taskId, agentId: 'owner' };
  expect(() => completeExternalPlanTask({ ...completion, agentId: 'intruder' })).toThrow(/not actively claimed/);
  expect(() => completeExternalPlanTask(completion)).toThrow(/observed check receipt/);
  expect(() => completeExternalPlanTask({ ...completion,
    receipt: { command: 'different test', status: 'SUCCESS', message: 'passed' } })).toThrow(/must match/);
  expect(() => completeExternalPlanTask({ ...completion,
    receipt: { command: 'yarn test adapter', status: 'SUCCESS', message: ' ' } })).toThrow(/message is required/);
  const store = openAwarenessStore({ workspace });
  try { expect(store.getTask(taskId)).toMatchObject({ status: 'IN_PROGRESS', agentId: 'owner' }); }
  finally { store.close(); }
  expect(completeExternalPlanTask({ ...completion,
    receipt: { command: 'yarn test adapter', status: 'FAILED', message: 'adapter assertion failed' } }))
    .toMatchObject({ verified: false, task: { status: 'FAILED', verifiedBy: 'owner', verificationMessage: 'adapter assertion failed' } });
});
