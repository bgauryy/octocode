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

test('Awareness tool effects follow the canonical operation catalog', () => {
  const query = (operation?: string, params?: Record<string, unknown>) => ({ queries: [{ ...(operation ? { operation } : {}), ...(params ? { params } : {}) }] });
  assert.equal(getToolEffect('awareness', query()), undefined);
  assert.equal(getToolEffect('awareness', query('context.orient')), 'read');
  assert.equal(getToolEffect('awareness', query('message.send', { kind: 'fyi', subject: 'status' })), 'coordination-write');
  assert.equal(getToolEffect('awareness', query('history.restore', { action: 'apply', preview_id: 'p1' })), 'workspace-write');
  assert.equal(getToolEffect('awareness', query('history.restore', { action: 'preview', operation_id: 'o1', side: 'before' })), 'read');
  assert.equal(getToolEffect('awareness', query('not.an.operation')), undefined);
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
  assert.equal(adoptPlanModePolicy(session, { phase: 'executing', branchSnapshotId: 'branch-b', generation: 2 }), true);
});
