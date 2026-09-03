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
  PLAN_MODE_BLOCK_REASON,
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

test('pre-Start policy allows planning/coordination/read and blocks workspace, external, shell, and unknown effects', () => {
  const session = ctx('review-session');
  enterPlanMode(session);
  assert.equal(isPlanMode(session), true);
  for (const allowed of ['plan', 'askUser', 'skill', 'claim', 'readMedia', 'web', 'localSearch']) {
    assert.equal(planModeToolGate(allowed, session), undefined, `${allowed} remains available during review`);
  }
  assert.deepEqual(
    planModeToolGate('agent', session),
    { block: true, reason: PLAN_MODE_BLOCK_REASON },
    'an agent call without a resolved batch fails closed',
  );
  assert.equal(
    planModeToolGate('skill', session, { queries: [{ reasoning: 'load RFC guidance', type: 'load', name: 'octocode-rfc-generator' }] }),
    undefined,
    'installed skill reads remain available during review',
  );
  for (const mode of ['auto', 'use', 'create', 'enhance', 'fix', 'list', 'delete']) {
    assert.deepEqual(
      planModeToolGate('skill', session, { queries: [{ reasoning: 'dynamic skill operation', type: 'call', skillType: 'release-flow', mode }] }),
      { block: true, reason: PLAN_MODE_BLOCK_REASON },
      `dynamic skill mode ${mode} is blocked because the orchestrator may mutate dynamic-skill storage`,
    );
  }
  assert.deepEqual(
    planModeToolGate('skill', session, {
      queries: [
        { reasoning: 'safe installed skill read', type: 'load', action: 'list' },
        { reasoning: 'unsafe dynamic skill mutation', type: 'call', skillType: 'release-flow', mode: 'create' },
      ],
    }),
    { block: true, reason: PLAN_MODE_BLOCK_REASON },
    'one mutating query blocks the entire ordered batch before execution',
  );
  assert.equal(planModeToolGate('MCPTool', session, { queries: [{ action: 'call', server: 'octocode' }] }), undefined);
  assert.ok(planModeToolGate('MCPTool', session, { queries: [{ action: 'status' }, { action: 'add', server: 'other' }] }));
  for (const blocked of ['file', 'edit', 'write', 'bash', 'createImage', 'chromeDebug', 'mysteryTool']) {
    assert.deepEqual(planModeToolGate(blocked, session), { block: true, reason: PLAN_MODE_BLOCK_REASON }, `${blocked} fails closed`);
  }
});

test('policies are isolated by session and only explicit off clears the targeted session', () => {
  const one = ctx('one');
  const two = ctx('two');
  enterPlanMode(one);
  assert.equal(isPlanMode(one), true);
  assert.equal(isPlanMode(two), false);
  assert.ok(planModeToolGate('edit', one));
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
  assert.ok(planModeToolGate('write', session), 'accepted still blocks mutation');
  assert.equal(adoptPlanModePolicy(session, { phase: 'executing', branchSnapshotId: 'branch-b', generation: 2 }), true);
  assert.equal(planModeToolGate('write', session), undefined, 'Start enables effects for the owning branch generation');
});
