import assert from 'node:assert/strict';
import { test } from 'vitest';
import type { PlanReadModelV1 } from '../src/tools/plan-read-model.js';
import type { RuntimeState } from '../src/tools/runtime-store.js';
import { deriveUxSnapshot } from '../src/tools/ux-snapshot.js';

test('missing session timestamps cannot become epoch-sized elapsed times', () => {
  const state = runtime();
  state.footer.sessionStartedAt = 0;
  const snapshot = deriveUxSnapshot({ now: 1_789_232_000_000, runtime: state, agents: [] });
  assert.equal(snapshot.session.elapsedMs, 0);
});

test('task completion alone is not a verification receipt', () => {
  const snapshot = deriveUxSnapshot({ runtime: runtime(), plan: plan(), agents: [] });
  assert.equal(snapshot.tasks[0]?.status, 'done');
  assert.equal(snapshot.tasks[0]?.verification, 'unverified');
});

test('stale coordination uses human durations and an executable details command', () => {
  const snapshot = deriveUxSnapshot({ now: 100_000, runtime: runtime(), agents: [],
    awareness: { unread: 0, observedAt: 1_331, staleAfterMs: 16_000 } });
  const warning = snapshot.attention.find(item => item.kind === 'stale_source')!;
  assert.match(warning.reason, /1m 39s/);
  assert.doesNotMatch(warning.reason, /ms/);
  assert.equal(warning.detailRoute, '/octocode-status');
});

test('worker updates prefer live tools and fresh output over old messages', () => {
  const worker = {
    agentId: 'worker', name: 'atlas', status: 'running',
    startedAt: '1970-01-01T00:00:01.000Z', updatedAt: '1970-01-01T00:00:09.000Z',
    lastMessage: { direction: 'to-agent' as const, action: 'send' as const, preview: 'old assignment', timestamp: 1000 },
    activeTool: 'localSearch', deltaSummary: 'Found the caller',
  };
  const current = (overrides = {}) => deriveUxSnapshot({ now:10000, runtime:runtime(), agents:[{...worker,...overrides}] }).agents[0]!;
  assert.equal(current().activeOperation, 'tool localSearch');
  assert.equal(current({ activeTool: undefined }).activeOperation, 'Found the caller');
  assert.equal(current({ status: 'idle', normalizedStatus: 'done' }).activeOperation, 'Found the caller');
  assert.match(current({ status: 'idle', pendingMessages: 1 }).activeOperation!, /old assignment/);
});

function plan(overrides: Partial<PlanReadModelV1> = {}): PlanReadModelV1 {
  return {
    version: 1,
    planId: 'plan-1',
    phase: 'executing',
    revision: 'rev-1',
    acceptedRevision: 'rev-1',
    summary: { total: 3, done: 1, running: 1, blocked: 0 },
    tasks: [
      { id: 't1', index: 1, text: 'Research', status: 'done', dependsOn: [] },
      { id: 't2', index: 2, text: 'Implement', activeText: 'Implementing', status: 'doing', dependsOn: [1] },
      { id: 't3', index: 3, text: 'Verify', status: 'todo', dependsOn: [2] },
    ],
    review: {
      branchSnapshotId: 'branch-1', generation: 1, blockingQuestions: 0,
      unresolvedComments: 0, decisions: [], questions: [], comments: [],
    },
    coordination: { mode: 'local', sourcePlanKey: 'plan-1', workspace: '/repo' },
    authorization: {},
    pendingInteractionIds: [],
    runtime: { turnsSinceUpdate: 0 },
    ...overrides,
    shape: overrides.shape ?? 'linear',
  };
}

function runtime(overrides: Partial<Pick<RuntimeState, 'generation' | 'phase' | 'activity' | 'context' | 'footer' | 'backgroundJobs'>> = {}) {
  return {
    generation: 2,
    phase: 'ready' as const,
    activity: { kind: 'working' as const, since: 9_000, planScope: 'plan-1', stepId: 't2', label: 'Implementing' },
    context: {
      status: 'ready' as const, mode: 'exact' as const, systemPromptChars: 1, mcpChars: 1,
      dynamicChars: 0, directToolChars: 0, providerSubtotalChars: 2, estimatedTokens: 1,
      mcpServers: 1, mcpTools: 2, skills: 3,
    },
    footer: {
      sessionStartedAt: 1_000, activeTurnStartedAt: 8_000, completedTurns: 2,
      usage: { tokens: 60, contextWindow: 100 }, githubAuth: { status: 'authenticated' as const },
    },
    backgroundJobs: [],
    ...overrides,
  };
}

test('derives a frozen linear snapshot and keeps worker completion separate from parent tasks', () => {
  const snapshot = deriveUxSnapshot({
    now: 10_000,
    runtime: runtime(),
    plan: plan(),
    agents: [{
      agentId: 'agent-1', name: 'atlas', status: 'idle', normalizedStatus: 'done',
      task: 'Implement', planStep: 't2', startedAt: '1970-01-01T00:00:08.000Z',
      updatedAt: '1970-01-01T00:00:09.500Z',
    }],
    goal: { text: 'Ship adaptive status', nextAction: 'Verify implementation' },
  });

  assert.equal(snapshot.version, 1);
  assert.equal(snapshot.session.elapsedMs, 9_000);
  assert.equal(snapshot.plan?.progressMode, 'linear');
  assert.deepEqual(snapshot.plan && {
    total: snapshot.plan.total,
    done: snapshot.plan.done,
    active: snapshot.plan.active,
    ready: snapshot.plan.ready,
    blocked: snapshot.plan.blocked,
  }, { total: 3, done: 1, active: 1, ready: 1, blocked: 0 });
  assert.equal(snapshot.agents[0]?.state, 'done');
  assert.equal(snapshot.tasks.find((task) => task.id === 't2')?.status, 'doing');
  assert.equal(snapshot.messages.queued, 0);
  assert.ok(Object.isFrozen(snapshot));
  assert.ok(Object.isFrozen(snapshot.tasks));
  assert.ok(Object.isFrozen(snapshot.tasks[0]));
});

test('marks parallel plans as graph progress and promotes input, failures, messages, context, and stale sources', () => {
  const parallel = plan({
    phase: 'needs_answers',
    shape: 'graph',
    pendingInteractionIds: ['interaction-1'],
    summary: { total: 4, done: 1, running: 1, blocked: 1 },
    tasks: [
      { id: 't1', index: 1, text: 'Research', status: 'done', dependsOn: [] },
      { id: 't2', index: 2, text: 'Build A', status: 'doing', dependsOn: [1] },
      { id: 't3', index: 3, text: 'Build B', status: 'todo', dependsOn: [1] },
      { id: 't4', index: 4, text: 'Ship', status: 'blocked', dependsOn: [2, 3] },
    ],
  });
  const snapshot = deriveUxSnapshot({
    now: 40_000,
    runtime: runtime({
      activity: { kind: 'awaiting_input', since: 35_000, planScope: 'plan-1', question: 'Start?' },
      context: { ...runtime().context, status: 'stale' },
      footer: { ...runtime().footer, usage: { tokens: 96, contextWindow: 100 } },
    }),
    plan: parallel,
    agents: [
      {
        agentId: 'blocked-1', name: 'atlas', status: 'idle', normalizedStatus: 'blocked',
        startedAt: '1970-01-01T00:00:01.000Z', updatedAt: '1970-01-01T00:00:39.000Z',
      },
      {
        agentId: 'queued-1', name: 'rhea', status: 'idle', normalizedStatus: 'done',
        pendingMessages: 2, startedAt: '1970-01-01T00:00:01.000Z', updatedAt: '1970-01-01T00:00:39.000Z',
      },
      {
        agentId: 'failed-1', name: 'nova', status: 'failed',
        startedAt: '1970-01-01T00:00:01.000Z', updatedAt: '1970-01-01T00:00:39.000Z',
      },
    ],
    awareness: { unread: 3, observedAt: 5_000, staleAfterMs: 10_000, latestSender: 'reviewer', latestSubject: 'Check failure' },
  });

  assert.equal(snapshot.plan?.progressMode, 'graph');
  assert.equal(snapshot.messages.queued, 2);
  assert.equal(snapshot.agents.find(agent => agent.id === 'queued-1')?.state, 'queued');
  assert.equal(snapshot.messages.unread, 3);
  assert.equal(snapshot.provenance.find((item) => item.owner === 'awareness')?.stale, true);
  assert.deepEqual(snapshot.attention.slice(0, 2).map((item) => item.priority), ['P0', 'P0']);
  assert.ok(snapshot.attention.some((item) => item.kind === 'agent_blocked' && item.actor === 'atlas'));
  assert.ok(snapshot.attention.some((item) => item.kind === 'agent_failed' && item.actor === 'nova'));
  assert.ok(snapshot.attention.some((item) => item.kind === 'context_pressure'));
  assert.ok(snapshot.attention.some((item) => item.kind === 'stale_source'));
});

test('dynamic plans never expose a fixed denominator', () => {
  const snapshot = deriveUxSnapshot({
    now: 10_000,
    runtime: runtime(),
    plan: plan(),
    agents: [],
    dynamicPlan: true,
  });
  assert.equal(snapshot.plan?.progressMode, 'dynamic');
  assert.equal(snapshot.plan?.displayTotal, undefined);
});

test('projects background work and promotes failed jobs without exposing process resources', () => {
  const snapshot = deriveUxSnapshot({
    now: 10_000,
    runtime: runtime({
      backgroundJobs: [
        { id: 'running', title: 'Typecheck', status: 'running', startedAt: 8_000, updatedAt: 9_000 },
        { id: 'failed', title: 'Build', status: 'failed', startedAt: 7_000, endedAt: 9_500, updatedAt: 9_500, exitCode: 1 },
      ],
    }),
  });

  assert.deepEqual(snapshot.backgroundJobs.map((job) => ({ id: job.id, elapsedMs: job.elapsedMs })), [
    { id: 'running', elapsedMs: 2_000 },
    { id: 'failed', elapsedMs: 2_500 },
  ]);
  assert.ok(snapshot.attention.some((item) => item.kind === 'background_failed' && item.reason.includes('Build')));
  assert.equal('pid' in snapshot.backgroundJobs[0]!, false);
  assert.equal('logPath' in snapshot.backgroundJobs[0]!, false);
});
