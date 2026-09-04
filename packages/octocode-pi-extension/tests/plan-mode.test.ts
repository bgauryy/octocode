import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';
import { OCTOCODE_SUPPORT_TOOL_NAMES, OVERRIDDEN_BUILTIN_TOOL_NAMES } from '../src/constants.js';
import {
  adoptPlanModePolicy,
  clearPlanModePoliciesForTests,
  enterPlanMode,
  exitPlanMode,
  getPlanModePolicy,
  getToolEffect,
  evaluateToolCapability,
  isPlanMode,
  planModeToolGate,
  unclassifiedToolNames,
} from '../src/tools/plan-mode.js';

function ctx(sessionId: string) {
  return {
    cwd: '/tmp/plan-policy-workspace',
    sessionManager: { getSessionId: () => sessionId },
  } as never;
}

afterEach(() => clearPlanModePoliciesForTests());

test('every shipped support/override tool has declared effect metadata', () => {
  const names = [...new Set([...OCTOCODE_SUPPORT_TOOL_NAMES, ...OVERRIDDEN_BUILTIN_TOOL_NAMES])];
  assert.deepEqual(unclassifiedToolNames(names), []);
  assert.equal(getToolEffect('plan'), 'planning-write');
  assert.equal(getToolEffect('lock'), 'coordination-write');
  assert.equal(getToolEffect('file'), 'workspace-write');
  assert.equal(getToolEffect('web'), 'read');
});

test('capability receipts are deterministic and deny precedence is fail-closed', () => {
  const input = { toolName: 'unknown-plugin-tool', phase: 'in_review' as const, createdAt: '2026-08-26T00:00:00.000Z' };
  const first = evaluateToolCapability(input);
  assert.deepEqual(evaluateToolCapability(input), first);
  assert.equal(first.effectiveDecision, 'block');
  assert.ok(first.guards.some((guard) => guard.name === 'tool-effect-classified' && guard.decision === 'block'));
});

test('pre-Start policy tracks the phase without blocking tool execution', () => {
  const session = ctx('review-session');
  enterPlanMode(session);
  assert.equal(isPlanMode(session), true);
  for (const [toolName, toolInput] of [
    ['plan', undefined],
    ['askUser', undefined],
    ['skill', { queries: [{ reasoning: 'create a workflow', type: 'call', skillType: 'release-flow', mode: 'create' }] }],
    ['file', undefined],
    ['bash', undefined],
    ['chromeDebug', undefined],
    ['agent', { queries: [{ type: 'spawn', profile: 'researcher' }] }],
    ['MCPTool', { queries: [{ action: 'add', server: 'other' }] }],
    ['mysteryTool', undefined],
  ] as const) {
    assert.equal(planModeToolGate(toolName, session, toolInput), undefined, `${toolName} is not restricted by plan phase`);
  }
  assert.equal(
    evaluateToolCapability({ toolName: 'file', phase: 'in_review', createdAt: '2026-08-26T00:00:00.000Z' }).effectiveDecision,
    'allow',
    'plan phase is audit context, not an execution deny',
  );
});

test('policies are isolated by session and only explicit off clears the targeted session', () => {
  const one = ctx('one');
  const two = ctx('two');
  enterPlanMode(one);
  assert.equal(isPlanMode(one), true);
  assert.equal(isPlanMode(two), false);
  assert.equal(planModeToolGate('edit', one), undefined);
  assert.equal(planModeToolGate('edit', two), undefined);
  exitPlanMode(two);
  assert.equal(isPlanMode(one), true, 'clearing another session cannot disable this gate');
  exitPlanMode(one);
  assert.equal(isPlanMode(one), false);
});

test('branch adoption replaces policy atomically and rejects stale same-branch generations', () => {
  const session = ctx('branch-session');
  assert.equal(adoptPlanModePolicy(session, { phase: 'in_review', branchSnapshotId: 'branch-a', generation: 4 }), true);
  assert.equal(adoptPlanModePolicy(session, { phase: 'draft', branchSnapshotId: 'branch-a', generation: 3 }), false);
  assert.deepEqual(getPlanModePolicy(session), {
    phase: 'in_review',
    branchSnapshotId: 'branch-a',
    generation: 4,
  });
  assert.equal(adoptPlanModePolicy(session, { phase: 'accepted', branchSnapshotId: 'branch-b', generation: 1 }), true);
  assert.equal(getPlanModePolicy(session)?.branchSnapshotId, 'branch-b', 'tree switch adopts the active branch even with a lower generation');
  assert.equal(planModeToolGate('write', session), undefined, 'accepted plans do not disable tools');
  assert.equal(adoptPlanModePolicy(session, { phase: 'executing', branchSnapshotId: 'branch-b', generation: 2 }), true);
  assert.equal(planModeToolGate('write', session), undefined, 'execution remains unrestricted by plan tracking');
});
