/**
 * Focused tests for plan-tool queries[] envelope contract.
 *
 * Covers: schema shape, per-query reasoning, preflight validation,
 * multi-query ordered execution, single-query detail passthrough,
 * flat-call rejection, and renderCall envelope awareness.
 */
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, test } from 'vitest';
import { Type } from 'typebox';
import { openAwareness } from '@octocodeai/octocode-awareness';
import type { ToolDefinition, PiContext } from '../src/types.js';
import { handleOctocodePlanCommand, registerPlanTool, setUnifiedPlanProjectorForTests } from '../src/tools/plan-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { completeUnifiedPlanTask } from '../src/tools/awareness-shared.js';
import { acceptPlanReview, clearPlan, getPlan, getPlanCoordination, getPlanReviewState, proposePlanReview, setPlan, setPlanRfc, updatePlanCoordination } from '../src/tools/active-plan.js';
import { setInteractionStoreFactoryForTests } from '../src/tools/interaction-broker.js';
import { getCurrentPlanReadModel } from '../src/tools/plan-read-model.js';

const CWD = '/tmp/plan-query-test-ws';

function loadTool(): ToolDefinition {
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (d: ToolDefinition) => tools.set(d.name, d) };
  registerPlanTool(pi, Type, new Set<string>(), registerUniqueTool);
  return tools.get('plan')!;
}

const ctx = { cwd: CWD } as unknown as PiContext;

afterEach(() => {
  clearPlan(CWD);
  setUnifiedPlanProjectorForTests();
});

// ─── Schema shape ────────────────────────────────────────────────────────────

test('plan schema exposes only queries[] at the top level', () => {
  const tool = loadTool();
  const schema = tool.parameters as {
    properties?: Record<string, unknown>;
    required?: string[];
    additionalProperties?: boolean;
  };
      assert.deepEqual(Object.keys(schema.properties ?? {}), ['queries', 'queryRunType'], 'queries and run policy present');
  assert.ok(schema.required?.includes('queries'), 'queries is required');
});

test('plan schema exposes session, shared, and auto scope on each query', () => {
  const tool = loadTool();
  const schema = tool.parameters as {
    properties?: { queries?: { items?: { properties?: Record<string, { enum?: string[] }> } } };
  };
  assert.deepEqual(schema.properties?.queries?.items?.properties?.['scope']?.enum, ['auto', 'session', 'shared']);
  assert.ok(schema.properties?.queries?.items?.properties?.['receipt'], 'complete exposes an observed check receipt');
});

test('plan schema requires reasoning on each query item', () => {
  const tool = loadTool();
  const schema = tool.parameters as {
    properties?: {
      queries?: {
        minItems?: number;
        items?: { properties?: Record<string, unknown>; required?: string[] };
      };
    };
  };
  const item = schema.properties?.queries?.items;
  assert.ok(item?.properties?.['reasoning'], 'reasoning property exists on item');
  assert.ok(item?.required?.includes('reasoning'), 'reasoning is required on item');
  assert.ok(item?.properties?.['action'], 'action property exists on item');
  assert.equal(schema.properties?.queries?.minItems, 1, 'minItems is 1');
});

test('plan schema discriminates actions and advertises their required fields', () => {
  const tool = loadTool();
  const schema = tool.parameters as {
    properties?: {
      queries?: {
        items?: {
          oneOf?: Array<{ title?: string; required?: string[] }>;
          properties?: Record<string, { minItems?: number; maxItems?: number; items?: { maxItems?: number } }>;
        };
      };
    };
  };
  const item = schema.properties?.queries?.items;
  assert.deepEqual(item?.oneOf?.map((branch) => branch.title), [
    'set', 'propose', 'clarify', 'add', 'start', 'complete', 'remove', 'clear', 'show',
  ]);
  assert.deepEqual(item?.oneOf?.find((branch) => branch.title === 'set')?.required, ['action', 'steps']);
  assert.deepEqual(item?.oneOf?.find((branch) => branch.title === 'clarify')?.required, ['action', 'questions']);
  assert.deepEqual(item?.oneOf?.find((branch) => branch.title === 'add')?.required, ['action', 'text']);
  assert.equal(item?.properties?.['steps']?.minItems, 1);
  assert.equal(item?.properties?.['questions']?.maxItems, 3);
});

// ─── Single-query passthrough ─────────────────────────────────────────────────

test('single set query returns original detail shape (steps, action) passthrough', async () => {
  const tool = loadTool();
  const result = await tool.execute(
    'id',
    { queries: [{ reasoning: 'set up the plan', action: 'set', steps: ['Step A', 'Step B'] }] },
    undefined, undefined, ctx,
  );
  assert.equal(result.isError, undefined, 'no error');
  const d = result.details as { action?: string; steps?: unknown[] };
  assert.equal(d?.action, 'set', 'details.action passthrough');
  assert.equal(d?.steps?.length, 2, 'details.steps passthrough');
});

test('single show query returns the canonical versioned RPC read model', async () => {
  const tool = loadTool();
  // First set up a plan
  await tool.execute('id', { queries: [{ reasoning: 'setup', action: 'set', steps: ['Alpha'] }] }, undefined, undefined, ctx);
  const result = await tool.execute('id', { queries: [{ reasoning: 'checking plan', action: 'show' }] }, undefined, undefined, ctx);
  const d = result.details as { steps?: unknown[]; plan?: { version?: number; phase?: string; tasks?: Array<{ id: string; status: string }> }; addendum?: string };
  assert.equal(d?.steps?.length, 1);
  assert.equal(d.plan?.version, 1);
  assert.equal(d.plan?.phase, 'executing');
  assert.deepEqual(d.plan?.tasks, d.steps);
  assert.match(d.addendum ?? '', /<active_plan>/);
});

test('unified auto and explicit session scopes keep solo plans out of Awareness', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-scope-session-'));
  const tool = loadTool();
  const localCtx = { cwd: workspace } as unknown as PiContext;
  try {
    for (const scope of ['auto', 'session'] as const) {
      const result = await tool.execute('id', {
        queries: [{
          reasoning: `exercise ${scope} scope`,
          action: 'set',
          scope,
          steps: [{ text: `${scope} task`, paths: ['src/a.ts'], acceptance: 'done', checkCommand: 'test' }],
        }],
      }, undefined, undefined, localCtx);
      assert.equal(result.isError, undefined);
      const lite = openAwareness({ workspace });
      try {
        assert.equal(lite.listPlans().length, 0, `${scope} created no shared plan`);
        assert.equal(lite.listTasks().length, 0, `${scope} created no shared task`);
      } finally {
        lite.close();
      }
    }
  } finally {
    clearPlan(workspace);
    rmSync(workspace, { recursive: true, force: true });
  }
});

test('unified shared scope idempotently projects step contracts and dependencies', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-scope-shared-'));
  const previousAgent = process.env['OCTOCODE_AGENT_ID'];
  process.env['OCTOCODE_AGENT_ID'] = 'pi:plan-shared-test';
  const tool = loadTool();
  const localCtx = { cwd: workspace } as unknown as PiContext;
  const query = {
    reasoning: 'project shared plan',
    action: 'set',
    scope: 'shared',
    steps: [
      { text: 'First shared task', paths: ['src/a.ts'], reasoning: 'first', acceptance: 'first done', checkCommand: 'test first' },
      { text: 'Second shared task', dependsOn: [1], paths: ['src/b.ts'], reasoning: 'second', acceptance: 'second done', checkCommand: 'test second' },
    ],
  };
  try {
    const first = await tool.execute('id', { queries: [query] }, undefined, undefined, localCtx);
    assert.equal(first.isError, undefined);
    const local = getPlan(workspace);
    assert.ok(local.every((step) => step.awarenessTaskId), 'every local step has a shared task mapping');

    const lite = openAwareness({ workspace });
    try {
      const plans = lite.listPlans();
      const tasks = lite.listTasks();
      assert.equal(plans.length, 1);
      assert.equal(tasks.length, 2);
      assert.deepEqual(tasks[0]?.paths, ['src/a.ts']);
      assert.equal(tasks[0]?.acceptance, 'first done');
      assert.equal(tasks[0]?.checkCommand, 'test first');
      assert.deepEqual(tasks[1]?.dependencies, [tasks[0]?.taskId]);
      assert.equal(tasks[0]?.status, 'CLAIMED', 'first runnable task is claimed for the current Pi agent');

      const second = await tool.execute('id', {
        queries: [{ reasoning: 'retry shared projection', action: 'start', scope: 'shared', index: 1 }],
      }, undefined, undefined, localCtx);
      assert.equal(second.isError, undefined);
      assert.equal(lite.listPlans().length, 1, 'retry reuses the sourced plan');
      assert.equal(lite.listTasks().length, 2, 'retry reuses sourced tasks');
    } finally {
      lite.close();
    }
  } finally {
    clearPlan(workspace);
    rmSync(workspace, { recursive: true, force: true });
    if (previousAgent === undefined) delete process.env['OCTOCODE_AGENT_ID'];
    else process.env['OCTOCODE_AGENT_ID'] = previousAgent;
  }
});

test('canonical read model follows peer-updated Awareness status while preserving local plan identity', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-shared-read-model-'));
  const localCtx = { cwd: workspace } as unknown as PiContext;
  const tool = loadTool();
  try {
    const result = await tool.execute('id', {
      queries: [{ reasoning: 'project shared task', action: 'set', scope: 'shared', steps: [{ text: 'Shared task', paths: ['src/a.ts'], acceptance: 'done', checkCommand: 'test' }] }],
    }, undefined, undefined, localCtx);
    assert.equal(result.isError, undefined);
    const localBefore = getPlan(workspace)[0]!;
    const identityBefore = getPlanReviewState(workspace);
    const lite = openAwareness({ workspace });
    const db = new DatabaseSync(lite.dbPath);
    db.prepare('UPDATE tasks SET status = ? WHERE task_id = ?').run('DONE', localBefore.awarenessTaskId!);
    db.close();
    lite.close();

    const model = getCurrentPlanReadModel(localCtx, workspace);
    assert.equal(model.tasks[0]?.status, 'done', 'newer shared terminal status wins over stale local doing state');
    assert.equal(getPlan(workspace)[0]?.status, 'doing', 'read reconciliation does not mutate branch-local storage');
    assert.deepEqual(
      { snapshot: model.review.branchSnapshotId, generation: model.review.generation },
      { snapshot: identityBefore.branchSnapshotId, generation: identityBefore.generation },
    );
  } finally {
    clearPlan(workspace);
    rmSync(workspace, { recursive: true, force: true });
  }
});

test('ordinary shared start restores the prior local status and mapping when projection fails', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-ordinary-start-compensation-'));
  const previousAgent = process.env['OCTOCODE_AGENT_ID'];
  process.env['OCTOCODE_AGENT_ID'] = 'pi:ordinary-start-compensation';
  const localCtx = { cwd: workspace } as unknown as PiContext;
  const tool = loadTool();
  try {
    const created = await tool.execute('id', {
      queries: [{
        reasoning: 'project two shared lanes', action: 'set', scope: 'shared',
        steps: [
          { text: 'First lane', paths: ['src/a.ts'], acceptance: 'first', checkCommand: 'test first' },
          { text: 'Second lane', paths: ['src/b.ts'], acceptance: 'second', checkCommand: 'test second' },
        ],
      }],
    }, undefined, undefined, localCtx);
    assert.equal(created.isError, undefined);
    const before = getPlan(workspace).map((step) => ({ ...step }));
    setUnifiedPlanProjectorForTests(() => { throw new Error('injected projection failure'); });

    const failed = await tool.execute('id', {
      queries: [{ reasoning: 'start the peer-conflicted lane', action: 'start', scope: 'shared', index: 2 }],
    }, undefined, undefined, localCtx);

    assert.equal(failed.isError, true);
    assert.match((failed.content[0] as { text: string }).text, /did not start|restored/i);
    assert.deepEqual(getPlan(workspace), before, 'status and stable Awareness mappings are restored exactly');
  } finally {
    setUnifiedPlanProjectorForTests();
    clearPlan(workspace);
    rmSync(workspace, { recursive: true, force: true });
    if (previousAgent === undefined) delete process.env['OCTOCODE_AGENT_ID'];
    else process.env['OCTOCODE_AGENT_ID'] = previousAgent;
  }
});

test('shared plan.complete requires a matching receipt, verifies success, and reopens failed checks without advancing locally', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-complete-shared-'));
  const previousAgent = process.env['OCTOCODE_AGENT_ID'];
  process.env['OCTOCODE_AGENT_ID'] = 'pi:plan-complete-test';
  const tool = loadTool();
  const localCtx = { cwd: workspace } as unknown as PiContext;
  try {
    await tool.execute('id', {
      queries: [{
        reasoning: 'create shared completion plan',
        action: 'set',
        scope: 'shared',
        steps: [
          { text: 'Checked task', paths: ['src/a.ts'], acceptance: 'checked', checkCommand: 'test checked' },
          { text: 'Dependent task', dependsOn: [1], paths: ['src/b.ts'], acceptance: 'dependent checked', checkCommand: 'test dependent' },
        ],
      }],
    }, undefined, undefined, localCtx);
    const [firstLocal, secondLocal] = getPlan(workspace);

    const missing = await tool.execute('id', {
      queries: [{ reasoning: 'complete without fabricated evidence', action: 'complete', index: 1 }],
    }, undefined, undefined, localCtx);
    assert.equal(missing.isError, true);
    assert.match((missing.content[0] as { text: string }).text, /receipt/i);
    assert.equal(getPlan(workspace)[0]?.status, 'doing', 'missing receipt does not advance local state');

    const success = await tool.execute('id', {
      queries: [{
        reasoning: 'record observed successful check',
        action: 'complete',
        index: 1,
        receipt: { command: 'test checked', status: 'SUCCESS', message: 'test checked passed' },
      }],
    }, undefined, undefined, localCtx);
    assert.equal(success.isError, undefined);
    assert.equal(getPlan(workspace)[0]?.status, 'done');
    assert.equal(getPlan(workspace)[1]?.status, 'doing', 'verified predecessor unlocks and claims dependent step');

    let lite = openAwareness({ workspace });
    try {
      const firstTask = lite.getTask(firstLocal!.awarenessTaskId!);
      const secondTask = lite.getTask(secondLocal!.awarenessTaskId!);
      assert.ok(firstTask.verifiedAt);
      assert.equal(firstTask.verificationMessage, 'test checked passed');
      assert.equal(secondTask.status, 'CLAIMED');
    } finally {
      lite.close();
    }

    const failed = await tool.execute('id', {
      queries: [{
        reasoning: 'record observed failed check',
        action: 'complete',
        index: 2,
        receipt: { command: 'test dependent', status: 'FAILED', message: 'test dependent failed' },
      }],
    }, undefined, undefined, localCtx);
    assert.equal(failed.isError, true);
    assert.match((failed.content[0] as { text: string }).text, /failed|reopened/i);
    assert.equal(getPlan(workspace)[1]?.status, 'doing', 'failed check leaves local step in progress');
    lite = openAwareness({ workspace });
    try {
      const reopened = lite.getTask(secondLocal!.awarenessTaskId!);
      assert.equal(reopened.status, 'CLAIMED');
      assert.equal(reopened.verificationMessage, 'test dependent failed');
    } finally {
      lite.close();
    }

    const retry = await tool.execute('id', {
      queries: [{
        reasoning: 'record successful retry',
        action: 'complete',
        index: 2,
        receipt: { command: 'test dependent', status: 'SUCCESS', message: 'test dependent passed on retry' },
      }],
    }, undefined, undefined, localCtx);
    assert.equal(retry.isError, undefined);
    assert.ok(getPlan(workspace).every((step) => step.status === 'done'));
    lite = openAwareness({ workspace });
    try {
      assert.equal(lite.listPlans()[0]?.status, 'DONE', 'shared plan closes after every task is verified');
    } finally {
      lite.close();
    }
  } finally {
    clearPlan(workspace);
    rmSync(workspace, { recursive: true, force: true });
    if (previousAgent === undefined) delete process.env['OCTOCODE_AGENT_ID'];
    else process.env['OCTOCODE_AGENT_ID'] = previousAgent;
  }
});

test('shared completion compensates a failed check mark and reports verification debt if reopen also fails', () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-complete-compensate-'));
  const agentId = 'pi:plan-compensation-test';
  try {
    let lite = openAwareness({ workspace });
    const plan = lite.createPlan({ title: 'Compensation plan' });
    const recoverable = lite.addTask({ planId: plan.planId, title: 'Recoverable', checkCommand: 'test recoverable' });
    lite.claimTask({ taskId: recoverable.taskId, agentId });
    const markFailure = new Error('injected mark failure');
    (lite as unknown as { markCheck: () => never }).markCheck = () => { throw markFailure; };
    assert.throws(
      () => completeUnifiedPlanTask({
        workspace,
        taskId: recoverable.taskId,
        agentId,
        receipt: { command: 'test recoverable', status: 'SUCCESS', message: 'passed' },
      }, () => lite),
      /task reopened.*injected mark failure/i,
    );

    lite = openAwareness({ workspace });
    assert.equal(lite.getTask(recoverable.taskId).status, 'CLAIMED', 'compensation restores an owned runnable task');
    const debt = lite.addTask({ planId: plan.planId, title: 'Debt', checkCommand: 'test debt' });
    lite.claimTask({ taskId: debt.taskId, agentId });
    (lite as unknown as { markCheck: () => never }).markCheck = () => { throw new Error('mark unavailable'); };
    (lite as unknown as { reopenTask: () => never }).reopenTask = () => { throw new Error('reopen unavailable'); };
    assert.throws(
      () => completeUnifiedPlanTask({
        workspace,
        taskId: debt.taskId,
        agentId,
        receipt: { command: 'test debt', status: 'SUCCESS', message: 'passed' },
      }, () => lite),
      /verification debt.*mark unavailable.*reopen unavailable/i,
    );

    lite = openAwareness({ workspace });
    try {
      const stranded = lite.getTask(debt.taskId);
      assert.equal(stranded.status, 'DONE');
      assert.equal(stranded.verifiedAt, null);
    } finally {
      lite.close();
    }
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
});

test('accepted RFC shared scope creates no Awareness rows until the separate Start command', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-scope-rfc-start-'));
  const previousAgent = process.env['OCTOCODE_AGENT_ID'];
  process.env['OCTOCODE_AGENT_ID'] = 'pi:plan-rfc-start-test';
  const rfcPath = join(workspace, '.octocode', 'rfc', 'demo', 'RFC.md');
  mkdirSync(join(workspace, '.octocode', 'rfc', 'demo'), { recursive: true });
  writeFileSync(rfcPath, '# Accepted design\n');
  const localCtx = { cwd: workspace } as unknown as PiContext;
  const notices: string[] = [];
  try {
    setPlan(workspace, [{ text: 'Implement accepted design', paths: ['src/a.ts'], acceptance: 'implemented', checkCommand: 'test' }], 'draft');
    updatePlanCoordination(workspace, { mode: 'required' });
    setPlanRfc(workspace, rfcPath);
    assert.equal(proposePlanReview(workspace).ok, true);
    assert.equal(acceptPlanReview(workspace, getPlanReviewState(workspace).revision!).ok, true);

    let lite = openAwareness({ workspace });
    try {
      assert.equal(lite.listPlans().length, 0, 'Accept creates no shared plan');
      assert.equal(lite.listTasks().length, 0, 'Accept creates no shared task');
    } finally {
      lite.close();
    }

    await handleOctocodePlanCommand(`start ${getPlanReviewState(workspace).acceptedRevision!}`, localCtx, (_ctx, message) => notices.push(message));
    lite = openAwareness({ workspace });
    try {
      assert.equal(lite.listPlans().length, 1, 'Start creates the shared plan');
      assert.equal(lite.listTasks().length, 1, 'Start creates the shared task');
      assert.equal(lite.listTasks()[0]?.status, 'CLAIMED');
      assert.ok(getPlan(workspace)[0]?.awarenessTaskId);
    } finally {
      lite.close();
    }
    assert.match(notices.join('\n'), /Implementation started/);
  } finally {
    clearPlan(workspace);
    rmSync(workspace, { recursive: true, force: true });
    if (previousAgent === undefined) delete process.env['OCTOCODE_AGENT_ID'];
    else process.env['OCTOCODE_AGENT_ID'] = previousAgent;
  }
});

test('failed shared Start consumes authority, restores acceptance, and retries with a fresh receipt over stable graph rows', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-start-compensation-'));
  const previousAgent = process.env['OCTOCODE_AGENT_ID'];
  process.env['OCTOCODE_AGENT_ID'] = 'pi:plan-start-compensation';
  const rfcPath = join(workspace, '.octocode', 'rfc', 'demo', 'RFC.md');
  mkdirSync(join(workspace, '.octocode', 'rfc', 'demo'), { recursive: true });
  writeFileSync(rfcPath, '# Accepted design\n');
  const localCtx = { cwd: workspace, mode: 'rpc' } as unknown as PiContext;
  const notices: string[] = [];
  try {
    setPlan(workspace, [{ text: 'Implement accepted design', paths: ['src/a.ts'], acceptance: 'implemented', checkCommand: 'test' }], 'draft');
    updatePlanCoordination(workspace, { mode: 'required' });
    setPlanRfc(workspace, rfcPath);
    assert.equal(proposePlanReview(workspace).ok, true);
    assert.equal(acceptPlanReview(workspace, getPlanReviewState(workspace).revision!).ok, true);

    const review = getPlanReviewState(workspace);
    assert.equal(getPlanReviewState(workspace).phase, 'accepted');
    const coordination = getPlanCoordination(workspace);
    const localStep = getPlan(workspace)[0]!;
    let lite = openAwareness({ workspace });
    const preexisting = lite.materializePlanGraph({
      sourceKind: 'pi',
      sourcePlanKey: coordination.sourcePlanKey,
      title: `Plan: ${localStep.text}`,
      goal: localStep.text,
      rfcPath,
      rfcRevision: review.acceptedRevision,
      steps: [{
        sourceStepKey: localStep.id,
        title: localStep.text,
        paths: localStep.paths,
        acceptance: localStep.acceptance,
        checkCommand: localStep.checkCommand,
        priority: 1,
      }],
    });
    const stablePlanId = preexisting.plan.planId;
    const stableTaskId = preexisting.tasks.get(localStep.id)!.taskId;
    lite.claimTask({ taskId: stableTaskId, agentId: 'peer-agent' });
    lite.close();

    setInteractionStoreFactoryForTests((storeWorkspace) => openAwareness({ workspace: storeWorkspace }));
    await handleOctocodePlanCommand(`start ${review.acceptedRevision!}`, localCtx, (_ctx, message) => notices.push(message));
    assert.equal(getPlanReviewState(workspace).phase, 'accepted', 'failed projection compensation restores accepted state');
    assert.deepEqual(getPlan(workspace).map((step) => step.status), ['todo']);
    assert.equal(getPlan(workspace)[0]?.awarenessTaskId, undefined, 'failed Start does not retain a local shared mapping');

    lite = openAwareness({ workspace });
    let db = new DatabaseSync(lite.dbPath);
    let consumed = db.prepare('SELECT receipt_id FROM authorization_receipts WHERE workspace_path = ? AND consumed_at IS NOT NULL ORDER BY created_at')
      .all(workspace) as Array<{ receipt_id: string }>;
    db.close();
    assert.equal(consumed.length, 1, `the failed Start authority remains consumed; notices=${notices.join(' | ')}`);
    const firstReceiptId = consumed[0]!.receipt_id;
    assert.throws(() => lite.consumeAuthorizationReceipt({
      receiptId: firstReceiptId,
      planId: coordination.sourcePlanKey,
      revision: review.acceptedRevision!,
      scope: 'plan.start',
    }), /already consumed/);
    assert.equal(lite.listPlans().length, 1);
    assert.equal(lite.listTasks().length, 1);
    assert.equal(lite.listPlans()[0]!.planId, stablePlanId);
    assert.equal(lite.listTasks()[0]!.taskId, stableTaskId);
    lite.releaseTask({ taskId: stableTaskId, agentId: 'peer-agent', blockedReason: 'allow authorized retry' });
    lite.close();

    await handleOctocodePlanCommand(`start ${review.acceptedRevision!}`, localCtx, (_ctx, message) => notices.push(message));
    assert.equal(getPlanReviewState(workspace).phase, 'executing');
    assert.equal(getPlan(workspace)[0]?.awarenessTaskId, stableTaskId, 'fresh Start reuses the stable graph task');
    lite = openAwareness({ workspace });
    db = new DatabaseSync(lite.dbPath);
    consumed = db.prepare('SELECT receipt_id FROM authorization_receipts WHERE workspace_path = ? AND consumed_at IS NOT NULL ORDER BY created_at')
      .all(workspace) as Array<{ receipt_id: string }>;
    db.close();
    assert.equal(consumed.length, 2);
    assert.equal(new Set(consumed.map((receipt) => receipt.receipt_id)).size, 2, 'retry consumes a fresh receipt');
    assert.equal(lite.listPlans()[0]!.planId, stablePlanId);
    assert.equal(lite.listTasks()[0]!.taskId, stableTaskId);
    lite.close();
    assert.match(notices.join('\n'), /acceptance was preserved/i);
    assert.match(notices.join('\n'), /Implementation started/i);
  } finally {
    setInteractionStoreFactoryForTests();
    clearPlan(workspace);
    rmSync(workspace, { recursive: true, force: true });
    if (previousAgent === undefined) delete process.env['OCTOCODE_AGENT_ID'];
    else process.env['OCTOCODE_AGENT_ID'] = previousAgent;
  }
});

test('unified auto scope adopts one current claimed task without manufacturing a plan', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-scope-adopt-'));
  const previousAgent = process.env['OCTOCODE_AGENT_ID'];
  process.env['OCTOCODE_AGENT_ID'] = 'pi:plan-adopt-test';
  const lite = openAwareness({ workspace });
  const sharedPlan = lite.createPlan({ title: 'Existing shared plan' });
  const sharedTask = lite.addTask({ planId: sharedPlan.planId, title: 'Existing shared task', paths: ['src/existing.ts'], acceptance: 'existing done', checkCommand: 'test existing' });
  lite.claimTask({ taskId: sharedTask.taskId, agentId: process.env['OCTOCODE_AGENT_ID'] });
  const tool = loadTool();
  const localCtx = { cwd: workspace } as unknown as PiContext;
  try {
    const result = await tool.execute('id', {
      queries: [{
        reasoning: 'adopt current shared ownership',
        action: 'set',
        scope: 'auto',
        steps: [{ text: 'Existing shared task', paths: ['src/existing.ts'], acceptance: 'existing done', checkCommand: 'test existing' }],
      }],
    }, undefined, undefined, localCtx);
    assert.equal(result.isError, undefined);
    assert.equal(getPlan(workspace)[0]?.awarenessTaskId, sharedTask.taskId);
    assert.equal(lite.listPlans().length, 1, 'adoption creates no second plan');
    assert.equal(lite.listTasks().length, 1, 'adoption creates no second task');
  } finally {
    lite.close();
    clearPlan(workspace);
    rmSync(workspace, { recursive: true, force: true });
    if (previousAgent === undefined) delete process.env['OCTOCODE_AGENT_ID'];
    else process.env['OCTOCODE_AGENT_ID'] = previousAgent;
  }
});

// ─── Multi-query ordered execution ───────────────────────────────────────────

test('multi-query set + start executes in order and returns aggregate result', async () => {
  const tool = loadTool();
  const result = await tool.execute(
    'multi-1',
    {
      queries: [
        { reasoning: 'define the plan', action: 'set', steps: ['First', 'Second'] },
        { reasoning: 'begin first step', action: 'start', index: 1 },
      ],
    },
    undefined, undefined, ctx,
  );
  assert.equal(result.isError, undefined);
  const d = result.details as { results?: Array<{ index: number; summary: string }> };
  assert.ok(Array.isArray(d?.results), 'aggregate results array present');
  assert.equal(d.results!.length, 2, 'two results');
  assert.equal(d.results![0]!.index, 0);
  assert.equal(d.results![1]!.index, 1);
  // The plan state should reflect ordered execution: First step doing
  const steps = getPlan(CWD);
  assert.equal(steps[0]!.status, 'doing', 'first step is doing after ordered set+start');
});

test('multi-query set + add executes in source order — two steps present', async () => {
  const tool = loadTool();
  const result = await tool.execute(
    'multi-2',
    {
      queries: [
        { reasoning: 'create plan', action: 'set', steps: ['Task X'] },
        { reasoning: 'add extra', action: 'add', text: 'Task Y' },
      ],
    },
    undefined, undefined, ctx,
  );
  assert.equal(result.isError, undefined);
  const steps = getPlan(CWD);
  assert.equal(steps.length, 2, 'two steps after ordered set+add');
  assert.equal(steps[0]!.text, 'Task X', 'first step is Task X');
  assert.equal(steps[1]!.text, 'Task Y', 'second step is Task Y');
});

test('multi-query set + start + complete: completeStep auto-advances next todo', async () => {
  const tool = loadTool();
  await tool.execute(
    'multi-2b',
    {
      queries: [
        { reasoning: 'create plan', action: 'set', steps: ['Task X', 'Task Y'] },
        { reasoning: 'start task x', action: 'start', index: 1 },
        { reasoning: 'complete task x', action: 'complete', index: 1 },
      ],
    },
    undefined, undefined, ctx,
  );
  const steps = getPlan(CWD);
  assert.equal(steps[0]!.status, 'done', 'Task X completed');
  // active-plan auto-advances the next todo when completing the only doing step
  assert.equal(steps[1]!.status, 'doing', 'Task Y auto-advanced to doing');
});

// ─── Preflight: action-specific validation before mutation ───────────────────

test('preflight rejects unknown action before any mutation', async () => {
  const tool = loadTool();
  await assert.rejects(
    () => tool.execute('id', { queries: [{ reasoning: 'do something', action: 'explode' }] }, undefined, undefined, ctx),
    /unknown plan action.*explode/i,
  );
  assert.equal(getPlan(CWD).length, 0, 'no mutation occurred');
});

test('preflight rejects add with empty text before mutation', async () => {
  const tool = loadTool();
  await assert.rejects(
    () => tool.execute('id', { queries: [{ reasoning: 'add something', action: 'add', text: '   ' }] }, undefined, undefined, ctx),
    /action:add requires/i,
  );
});

test('preflight rejects action-irrelevant fields before mutating an earlier query', async () => {
  const tool = loadTool();
  await assert.rejects(
    () => tool.execute('id', {
      queries: [
        { reasoning: 'would create a plan', action: 'set', steps: ['Step A'] },
        { reasoning: 'invalid show payload', action: 'show', text: 'not valid for show' },
      ],
    }, undefined, undefined, ctx),
    /action:show does not accept text/i,
  );
  assert.deepEqual(getPlan(CWD), [], 'full batch preflight prevents the earlier set');
});

test('preflight rejects non-integer index before mutation', async () => {
  const tool = loadTool();
  await assert.rejects(
    () => tool.execute('id', { queries: [{ reasoning: 'start step', action: 'start', index: 0 }] }, undefined, undefined, ctx),
    /index must be a positive integer/i,
  );
});

test('preflight rejects steps as non-array before any mutation', async () => {
  const tool = loadTool();
  await assert.rejects(
    () => tool.execute('id', { queries: [{ reasoning: 'set plan', action: 'set', steps: 'not-an-array' }] }, undefined, undefined, ctx),
    /steps must be an array/i,
  );
  assert.equal(getPlan(CWD).length, 0, 'no mutation occurred');
});

test('preflight stops batch before first query executes when second query is invalid', async () => {
  const tool = loadTool();
  await assert.rejects(
    () => tool.execute(
      'pre-2',
      {
        queries: [
          { reasoning: 'set plan first', action: 'set', steps: ['Step A'] },
          { reasoning: 'bad action second', action: 'kaboom' },
        ],
      },
      undefined, undefined, ctx,
    ),
    /unknown plan action|queries\[1\] failed preflight/i,
  );
  // Both queries are preflighted before execution; no mutation should occur
  assert.equal(getPlan(CWD).length, 0, 'preflight stops before first mutation');
});

test('missing reasoning on envelope query throws before execution', async () => {
  const tool = loadTool();
  await assert.rejects(
    () => tool.execute('id', { queries: [{ action: 'show' }] }, undefined, undefined, ctx),
    /reasoning/i,
  );
});

test('flat params without queries[] are rejected', async () => {
  const tool = loadTool();
  await assert.rejects(
    () => tool.execute('id', { action: 'set', steps: ['Simple task'] } as Record<string, unknown>, undefined, undefined, ctx),
    /queries/i,
  );
});

// ─── renderCall envelope awareness ───────────────────────────────────────────

test('renderCall reads action from queries[0]', () => {
  const tool = loadTool();
  const rendered = tool.renderCall?.(
    { queries: [{ reasoning: 'set plan', action: 'set', steps: ['A', 'B', 'C'] }] },
    undefined,
  );
  const output = rendered?.render(80).join('') ?? '';
  assert.match(output, /plan/i);
  assert.match(output, /set/);
  assert.match(output, /3/); // step count
});

test('renderCall shows every operation and its reasoning for multi-query calls', () => {
  const tool = loadTool();
  const rendered = tool.renderCall?.(
    {
      queries: [
        { reasoning: 'set', action: 'set', steps: ['A'] },
        { reasoning: 'start', action: 'start' },
        { reasoning: 'complete', action: 'complete' },
      ],
    },
    undefined,
  );
  const lines = rendered?.render(120) ?? [];
  assert.equal(lines.length, 7);
  assert.match(lines[0]!, /3 queries.*sequential/);
  assert.match(lines[1]!, /set/);
  assert.match(lines[2]!, /set/);
  assert.match(lines[3]!, /start/);
  assert.match(lines[4]!, /start/);
  assert.match(lines[5]!, /complete/);
  assert.match(lines[6]!, /complete/);
  assert.doesNotMatch(lines.join('\n'), /\+2|why:|reasoning:/i);
});
