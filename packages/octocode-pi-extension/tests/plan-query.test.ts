/**
 * Focused tests for plan-tool queries[] envelope contract.
 *
 * Covers: schema shape, optional batch labels, preflight validation,
 * multi-query ordered execution, single-query detail passthrough,
 * flat-call rejection, and renderCall envelope awareness.
 */
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, test } from 'vitest';
import type { ToolDefinition, PiContext } from '../src/types.js';
import { registerPlanTool } from '../src/tools/planning/plan-registration.js';

import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { activePlanScope, clearPlan, getPlan, getPlanCoordination, getPlanReviewState, setPlan, setPlanEntryAppender, setPlanRfc } from '../src/tools/planning/plan-store.js';
import { acceptPlanReview, proposePlanReview } from '../src/tools/planning/plan-lifecycle.js';

import { runtimeStoreFor } from '../src/tools/runtime-renderer.js';
import { createSessionArtifactContext } from '../src/tools/session-artifacts.js';
import { SESSION_AUDIT_RELATIVE_PATH } from '../src/tools/session-audit.js';

const CWD = '/tmp/plan-query-test-ws';

function loadTool(): ToolDefinition {
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (d: ToolDefinition) => tools.set(d.name, d) };
  registerPlanTool(pi, new Set<string>(), registerUniqueTool);
  return tools.get('plan')!;
}

const ctx = { cwd: CWD } as unknown as PiContext;

afterEach(() => {
  setPlanEntryAppender(null);
  clearPlan(CWD);
});

test('tool results surface authoritative persistence failures', async () => {
  setPlanEntryAppender(() => { throw new Error('disk unavailable'); });
  const result = await loadTool().execute('id', {
    queries: [{ action: 'set', steps: ['memory-only step'] }],
  }, undefined, undefined, ctx) as { content: Array<{ type: string; text?: string }>; details?: { persistenceWarning?: string } };
  assert.match(result.content.map((part) => part.text ?? '').join('\n'), /will not survive session recovery/);
  assert.equal(result.details?.persistenceWarning, 'disk unavailable');
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

test('plan guidance stays behavioral and leaves call shape to the schema', () => {
  const tool = loadTool();
  const guidance = [tool.description, tool.promptSnippet, ...(tool.promptGuidelines ?? [])].join('\n');
  assert.match(guidance, /set for authorized execution and propose when review is required/i);
  assert.match(guidance, /Complete only after the declared check succeeds/i);
  assert.doesNotMatch(guidance, /queries.*reasoning/is);
  assert.doesNotMatch(guidance, /plan\((?:set|propose|clarify|add|start|complete|remove|clear|show)(?::[^)]*)?\)/i);
});

type PlanSchemaBranch = {
  properties?: Record<string, { const?: string; enum?: string[]; minItems?: number; maxItems?: number }>;
  required?: string[];
};

function planSchemaBranches(tool: ToolDefinition): PlanSchemaBranch[] {
  const schema = tool.parameters as { properties?: { queries?: { items?: { anyOf?: PlanSchemaBranch[]; oneOf?: PlanSchemaBranch[] } } } };
  const items = schema.properties?.queries?.items;
  return items?.anyOf ?? items?.oneOf ?? [];
}

test('plan schema exposes scope and receipts only on matching action branches', () => {
  const branches = planSchemaBranches(loadTool());
  const set = branches.find((branch) => branch.properties?.['action']?.enum?.[0] === 'set')!;
  assert.deepEqual(set.properties?.['scope']?.enum, ['auto', 'session', 'shared']);
  const complete = branches.find((branch) => branch.properties?.['action']?.enum?.[0] === 'complete')!;
  assert.ok(complete.properties?.['receipt']);
  assert.equal(set.properties?.['receipt'], undefined);
});

test('plan schema keeps bounded reasoning optional on every action branch', () => {
  const tool = loadTool();
  const schema = tool.parameters as { properties?: { queries?: { minItems?: number } } };
  assert.equal(schema.properties?.queries?.minItems, 1);
  const branches = planSchemaBranches(tool);
  assert.equal(branches.length, 10);
  for (const branch of branches) {
    assert.ok(!branch.required?.includes('reasoning'));
    assert.ok(branch.properties?.['reasoning']);
    assert.ok(branch.required?.includes('action'));
  }
});

test('plan schema discriminates actions and advertises required branch fields', () => {
  const branches = planSchemaBranches(loadTool());
  assert.deepEqual(
    branches.map((branch) => branch.properties?.['action']?.enum?.[0]),
    ['set', 'propose', 'clarify', 'add', 'start', 'start', 'complete', 'remove', 'clear', 'show'],
  );
  const set = branches[0]!;
  const clarify = branches[2]!;
  assert.ok(set.required?.includes('steps'));
  assert.equal(set.properties?.['steps']?.minItems, 1);
  assert.ok(clarify.required?.includes('questions'));
  assert.equal(clarify.properties?.['questions']?.maxItems, 3);
});

test('plan schema separates step and reviewed Start into executable variants', () => {
  const starts = planSchemaBranches(loadTool())
    .filter((branch) => branch.properties?.['action']?.enum?.[0] === 'start');
  assert.equal(starts.length, 2);

  const stepStart = starts.find((branch) => branch.properties?.['index']);
  const reviewedStart = starts.find((branch) => branch.properties?.['revision']);
  assert.ok(stepStart);
  assert.ok(reviewedStart);
  assert.equal(stepStart.properties?.['revision'], undefined);
  assert.equal(stepStart.properties?.['authorizationInteractionId'], undefined);
  assert.equal(reviewedStart.properties?.['index'], undefined);
  assert.ok(reviewedStart.required?.includes('revision'));
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

test('successful plan mutations append session audit rows while show stays read-only', async () => {
  const root = mkdtempSync(join(tmpdir(), 'plan-audit-'));
  const workspace = join(root, 'workspace');
  const home = join(root, 'home');
  mkdirSync(workspace);
  const priorHome = process.env['OCTOCODE_HOME'];
  process.env['OCTOCODE_HOME'] = home;
  const auditCtx = {
    cwd: workspace,
    sessionManager: { getSessionId: () => 'plan-audit-session' },
  } as unknown as PiContext;
  try {
    const tool = loadTool();
    await tool.execute('id', { queries: [{ reasoning: 'setup', action: 'set', steps: ['Alpha'] }] }, undefined, undefined, auditCtx);
    await tool.execute('id', { queries: [{ reasoning: 'inspect', action: 'show' }] }, undefined, undefined, auditCtx);
    await tool.execute('id', { queries: [{ reasoning: 'cleanup', action: 'clear' }] }, undefined, undefined, auditCtx);

    const artifact = createSessionArtifactContext(auditCtx);
    const audit = readFileSync(artifact.resolve(SESSION_AUDIT_RELATIVE_PATH), 'utf8');
    assert.match(audit, /\| plan\.set \|/);
    assert.match(audit, /\| plan\.clear \|/);
    assert.doesNotMatch(audit, /\| plan\.show \|/);
  } finally {
    clearPlan(workspace);
    if (priorHome === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = priorHome;
    rmSync(root, { recursive: true, force: true });
  }
});

test('memory-only mode keeps auto plans local and rejects durable shared projection', async () => {
  const root = mkdtempSync(join(tmpdir(), 'plan-memory-only-'));
  const previousHome = process.env['OCTOCODE_HOME'];
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  process.env['OCTOCODE_HOME'] = root;
  process.env['OCTOCODE_STORAGE_MODE'] = 'memory';
  const workspace = join(root, 'repo');
  mkdirSync(workspace);
  const localCtx = { cwd: workspace } as PiContext;
  const tool = loadTool();
  try {
    const execute = (scope: string) => tool.execute('storage', { queries: [{
      reasoning: 'check storage policy', action: 'set', scope,
      steps: [{ text: `${scope} file review`, paths: ['a.ts'], acceptance: 'file reviewed', checkCommand: 'test' }],
    }] }, undefined, undefined, localCtx);
    const local = await execute('auto');
    assert.equal(local.isError, undefined);
    assert.equal(existsSync(join(root, 'awareness')), false, 'auto scope must not open a durable store');
    const before = structuredClone({ plan: getPlan(workspace), coordination: getPlanCoordination(workspace), review: getPlanReviewState(workspace) });
    await assert.rejects(execute('shared'), /Shared plans are retired/);
    assert.deepEqual({ plan: getPlan(workspace), coordination: getPlanCoordination(workspace), review: getPlanReviewState(workspace) }, before, 'rejected shared request must preserve the existing local plan');
    assert.equal(existsSync(join(root, 'awareness')), false, 'shared scope must not create a durable store');
  } finally {
    clearPlan(workspace);
    if (previousHome === undefined) delete process.env['OCTOCODE_HOME']; else process.env['OCTOCODE_HOME'] = previousHome;
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE']; else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
    rmSync(root, { recursive: true, force: true });
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

test('missing batch label is accepted', async () => {
  const tool = loadTool();
  const result = await tool.execute('id', { queries: [{ action: 'show' }] }, undefined, undefined, ctx);
  assert.equal(result.isError ?? false, false);
});

test('flat params without queries[] are rejected', async () => {
  const tool = loadTool();
  await assert.rejects(
    () => tool.execute('id', { action: 'set', steps: ['Simple task'] } as Record<string, unknown>, undefined, undefined, ctx),
    /queries/i,
  );
});

test('revision-only reviewed Start cannot bypass in-review or unreceipted accepted states', async () => {
  const workspace = mkdtempSync(join(tmpdir(), 'plan-start-boundary-'));
  const rfcPath = join(workspace, '.octocode', 'rfc', 'demo', 'RFC.md');
  mkdirSync(join(workspace, '.octocode', 'rfc', 'demo'), { recursive: true });
  writeFileSync(rfcPath, '# Authorization boundary\n');
  const localCtx = { cwd: workspace, mode: 'tui' } as unknown as PiContext;
  const scope = activePlanScope(localCtx);
  const tool = loadTool();
  try {
    setPlan(scope, [{ text: 'Protected implementation' }], 'draft');
    setPlanRfc(scope, rfcPath);
    assert.equal(proposePlanReview(scope).ok, true);
    const revision = getPlanReviewState(scope).revision!;

    const inReview = await failedToolResult(tool.execute('id', {
      queries: [{ reasoning: 'must not infer approval', action: 'start', revision }],
    }, undefined, undefined, localCtx)) as { isError?: boolean; details?: { error?: string } };
    assert.equal(inReview.isError, true);
    assert.equal(inReview.details?.error, 'authorization-required');
    assert.equal(getPlanReviewState(scope).phase, 'in_review');

    assert.equal(acceptPlanReview(scope, revision).ok, true);
    const unreceipted = await failedToolResult(tool.execute('id', {
      queries: [{ reasoning: 'must require persisted authority', action: 'start', revision }],
    }, undefined, undefined, localCtx)) as { isError?: boolean; details?: { error?: string } };
    assert.equal(unreceipted.isError, true);
    assert.equal(unreceipted.details?.error, 'authorization-required');
    assert.equal(getPlanReviewState(scope).phase, 'accepted');
  } finally {
    clearPlan(scope);
    rmSync(workspace, { recursive: true, force: true });
  }
});

test('review-phase and draft step actions fail instead of reporting a no-op success', async () => {
  const tool = loadTool();
  setPlan(CWD, ['A'], 'draft');
  const started = await failedToolResult(tool.execute('id', {
    queries: [{ reasoning: 'must not bypass review', action: 'start', index: 1 }],
  }, undefined, undefined, ctx)) as { isError?: boolean; details?: { error?: string } };
  assert.equal(started.isError, true);
  assert.equal(started.details?.error, 'authorization-required');
  assert.equal(getPlan(CWD)[0]?.status, 'todo');

  const completed = await failedToolResult(tool.execute('id', {
    queries: [{ reasoning: 'must not complete before Start', action: 'complete', index: 1 }],
  }, undefined, undefined, ctx)) as { isError?: boolean; details?: { error?: string } };
  assert.equal(completed.isError, true);
  assert.equal(completed.details?.error, 'phase-not-executing');
});

test('reviewed Start fields fail during execution instead of starting a step', async () => {
  const tool = loadTool();
  await tool.execute('id', {
    queries: [{ reasoning: 'set up active execution', action: 'set', steps: ['A', 'B', 'C'] }],
  }, undefined, undefined, ctx);
  await tool.execute('id', {
    queries: [{ reasoning: 'finish the active step', action: 'complete', index: 1 }],
  }, undefined, undefined, ctx);
  assert.deepEqual(getPlan(CWD).map((step) => step.status), ['done', 'doing', 'todo']);

  const started = await failedToolResult(tool.execute('id', {
    queries: [{ reasoning: 'must not reinterpret reviewed fields', action: 'start', revision: 'stale-review' }],
  }, undefined, undefined, ctx)) as { isError?: boolean; details?: { error?: string } };
  assert.equal(started.isError, true);
  assert.equal(started.details?.error, 'wrong-start-variant');
  assert.deepEqual(getPlan(CWD).map((step) => step.status), ['done', 'doing', 'todo']);
});

test('set activates the first dependency-ready step rather than a blocked first row', async () => {
  const tool = loadTool();
  await tool.execute('id', {
    queries: [{
      reasoning: 'exercise dependency-aware activation',
      action: 'set',
      steps: [{ text: 'Blocked first', dependsOn: [2] }, 'Runnable second'],
    }],
  }, undefined, undefined, ctx);
  assert.deepEqual(getPlan(CWD).map((step) => step.status), ['todo', 'doing']);
});

test('consequential proposals require an RFC unless an explicit justified override is supplied', async () => {
  const tool = loadTool();
  const riskySteps = ['Migrate the public API schema without backward compatibility'];
  const result = await failedToolResult(tool.execute('id', {
    queries: [{
      reasoning: 'exercise inferred consequential review',
      action: 'propose',
      steps: riskySteps,
    }],
  }, undefined, undefined, ctx)) as { isError?: boolean; details?: { error?: string } };
  assert.equal(result.isError, true);
  assert.equal(result.details?.error, 'rfc-required');

  const unjustified = await failedToolResult(tool.execute('id', {
    queries: [{ action: 'propose', steps: riskySteps, consequential: false }],
  }, undefined, undefined, ctx)) as { isError?: boolean; details?: { error?: string } };
  assert.equal(unjustified.isError, true);
  assert.equal(unjustified.details?.error, 'override-reason-required');

  const overridden = await tool.execute('id', {
    queries: [{
      reasoning: 'record the explicit local-only exception',
      action: 'propose',
      steps: riskySteps,
      consequential: false,
      reason: 'The fixture uses no persistent data or published contract despite the wording.',
    }],
  }, undefined, undefined, ctx) as { isError?: boolean };
  assert.notEqual(overridden.isError, true);
});

test('step count and benign cleanup terms do not force an RFC', async () => {
  const tool = loadTool();
  const result = await tool.execute('id', {
    queries: [{
      action: 'propose',
      steps: ['Delete stale test fixture', 'Rename helper', 'Update imports', 'Run tests', 'Document result'],
    }],
  }, undefined, undefined, ctx) as { isError?: boolean };
  assert.notEqual(result.isError, true);
});

test('proposal validation failures settle activity instead of leaving Creating plan stuck', async () => {
  const tool = loadTool();
  const invalidCtx = { cwd: '/tmp/plan-invalid-rfc-activity' } as unknown as PiContext;
  const invalid = await failedToolResult(tool.execute('id', {
    queries: [{ reasoning: 'exercise invalid RFC cleanup', action: 'propose', rfcPath: '../outside.md', steps: ['A'] }],
  }, undefined, undefined, invalidCtx)) as { isError?: boolean };
  assert.equal(invalid.isError, true);
  const invalidActivity = runtimeStoreFor(invalidCtx)?.getState().activity;
  assert.ok(!invalidActivity || !('detail' in invalidActivity) || invalidActivity.detail !== 'Creating plan…');

  clearPlan(invalidCtx.cwd!);
});

test('plan result renderer preserves failures and distinguishes an empty show from clear', () => {
  const tool = loadTool();
  const failure = tool.renderResult?.({
    content: [{ type: 'text', text: '[PLAN] invalid RFC' }],
    isError: true,
    details: { action: 'propose', error: 'rfc-gate' },
  }, {}, undefined)?.render(80).join('\n') ?? '';
  assert.match(failure, /invalid RFC/i);
  assert.doesNotMatch(failure, /cleared/i);

  const empty = tool.renderResult?.({
    content: [{ type: 'text', text: '[PLAN] no active plan' }],
    details: { action: 'show', steps: [] },
  }, {}, undefined)?.render(80).join('\n') ?? '';
  assert.match(empty, /no active plan/i);
  assert.doesNotMatch(empty, /cleared/i);
});

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
import { failedToolResult } from './helpers/failed-tool-result.js';
