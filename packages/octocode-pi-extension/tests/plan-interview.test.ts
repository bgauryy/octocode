/**
 * Tests for the plan(clarify) interview phase. runAskPrompt is mocked so we can
 * script the user's answers and assert they land in the durable decision log.
 */
import assert from 'node:assert/strict';
import { afterEach, test, vi } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition, PiContext } from '../src/types.js';

// A scripted outcome queue + a record of the questions / pagination metadata
// actually shown, hoisted so the vi.mock factory can close over them.
const { outcomes, asked, paginations } = vi.hoisted(() => ({
  outcomes: [] as unknown[],
  asked: [] as string[],
  paginations: [] as Array<{ current: number; total: number } | undefined>,
}));
vi.mock('../src/tools/ask-user-tool.js', () => ({
  runAskPrompt: async (_ctx: unknown, params: { question: string; pagination?: { current: number; total: number } }) => {
    asked.push(params.question);
    paginations.push(params.pagination);
    return outcomes.shift();
  },
}));

import { registerPlanTool } from '../src/tools/plan-tool.js';
import { getPlanDecisions, clearPlan, getPlanReviewState } from '../src/tools/active-plan.js';
import { runtimeStoreFor } from '../src/tools/runtime-renderer.js';
import type { ForegroundActivity } from '../src/tools/runtime-store.js';

function loadTool(): ToolDefinition {
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (d: ToolDefinition) => tools.set(d.name, d) };
  registerPlanTool(pi, Type, new Set<string>(), (p, n, d) => { n.add(d.name); p.registerTool?.(d); });
  return tools.get('plan')!;
}

const CWD = '/tmp/plan-interview-ws';
afterEach(() => { outcomes.length = 0; asked.length = 0; paginations.length = 0; clearPlan(CWD); });

async function clarify(questions: unknown): Promise<{ content: Array<{ text: string }>; details?: Record<string, unknown>; isError?: boolean; activity?: ForegroundActivity }> {
  const tool = loadTool();
  const ctx = { cwd: CWD } as unknown as PiContext; // hasUI falsy → skip panel; runAskPrompt is mocked
  const result = (await tool.execute('id', {
    queries: [{ action: 'clarify', questions, reasoning: 'collect plan decisions in this test' }],
  }, undefined, undefined, ctx)) as { content: Array<{ text: string }>; details?: Record<string, unknown>; isError?: boolean };
  return { ...result, activity: runtimeStoreFor(ctx)?.getState().activity };
}

test('plan(clarify) records selected (label) and free-text answers into the decision log', async () => {
  outcomes.push({ status: 'selected', value: 'sqlite', label: 'SQLite' });
  outcomes.push({ status: 'text', value: 'reuse existing auth' });
  const res = await clarify([
    { prompt: 'Storage backend?', options: [{ label: 'SQLite', value: 'sqlite', recommended: true }] },
    { prompt: 'Auth approach?' },
  ]);
  assert.notEqual(res.isError, true);
  assert.deepEqual(getPlanDecisions(CWD), [
    { q: 'Storage backend?', a: 'SQLite' },
    { q: 'Auth approach?', a: 'reuse existing auth' },
  ]);
  assert.match(res.content[0]!.text, /recorded 2 decision/);
  assert.match(res.content[0]!.text, /decision-complete/);
  assert.equal(res.activity?.kind, 'planning', 'the foreground returns to Planning immediately after the final answer');
});

test('plan(clarify) passes pagination metadata for multi-question interviews and keeps the clean prompt in both the overlay and the log', async () => {
  outcomes.push({ status: 'selected', label: 'X', value: 'x' });
  outcomes.push({ status: 'selected', label: 'Y', value: 'y' });
  await clarify([{ prompt: 'First?' }, { prompt: 'Second?' }]);
  // Numbering is now expressed via the pagination badge in the overlay header
  // rather than a prefix in the question string, so the question text is clean.
  assert.deepEqual(asked, ['First?', 'Second?'], 'question text is the clean prompt (numbering is in params.pagination)');
  assert.deepEqual(paginations, [{ current: 1, total: 2 }, { current: 2, total: 2 }], 'pagination metadata is threaded to the overlay');
  assert.deepEqual(getPlanDecisions(CWD).map((d) => d.q), ['First?', 'Second?'], 'the decision log also keeps the clean prompt');
});

test('plan(clarify) does not number a single-question interview', async () => {
  outcomes.push({ status: 'selected', label: 'X', value: 'x' });
  await clarify([{ prompt: 'Only one?' }]);
  assert.deepEqual(asked, ['Only one?']);
});

test('plan(clarify) caps the interview at 3 questions', async () => {
  for (let i = 0; i < 5; i++) outcomes.push({ status: 'selected', label: `A${i}`, value: `a${i}` });
  await clarify(Array.from({ length: 5 }, (_v, i) => ({ prompt: `Q${i}?` })));
  assert.equal(getPlanDecisions(CWD).length, 3, 'only the first 3 questions are asked');
});

test('plan(clarify) with no questions is an actionable error', async () => {
  const res = await clarify([]);
  assert.equal(res.isError, true);
  assert.match(res.content[0]!.text, /questions\[\] list/);
  assert.equal(getPlanDecisions(CWD).length, 0);
});

test('plan(clarify) halts on cancel and keeps only prior answers', async () => {
  outcomes.push({ status: 'selected', label: 'Yes', value: 'yes' });
  outcomes.push({ status: 'cancelled' });
  const res = await clarify([{ prompt: 'First?' }, { prompt: 'Second?' }, { prompt: 'Third?' }]);
  assert.deepEqual(getPlanDecisions(CWD), [{ q: 'First?', a: 'Yes' }], 'only the answered question is recorded');
  assert.match(res.content[0]!.text, /cancelled/i);
  assert.equal(getPlanReviewState(CWD).phase, 'draft', 'cancellation returns to a stable draft');
  assert.equal(res.activity?.kind, 'planning', 'footer no longer claims input is still pending');
});

test('plan(clarify) exposes a durable pending interaction instead of losing RPC correlation', async () => {
  outcomes.push({
    status: 'pending',
    interaction: {
      version: 1,
      interactionId: 'interaction-1',
      correlationId: 'correlation-1',
      sessionId: 'session-1',
    },
  });
  const res = await clarify([{ prompt: 'Which database?' }]);

  assert.match(res.content[0]!.text, /pending/i);
  assert.match(res.content[0]!.text, /correlation-1/);
  assert.deepEqual(res.details?.pendingInteraction, {
    version: 1,
    interactionId: 'interaction-1',
    correlationId: 'correlation-1',
    sessionId: 'session-1',
  });
  assert.deepEqual(getPlanDecisions(CWD), []);
});

test('plan(propose) remains draft and exposes durable approval correlation', async () => {
  outcomes.push({
    status: 'pending',
    interaction: {
      version: 1,
      interactionId: 'approval-1',
      correlationId: 'approval-correlation-1',
      sessionId: 'session-1',
    },
  });
  const tool = loadTool();
  const ctx = { cwd: CWD } as unknown as PiContext;
  const res = await tool.execute('id', {
    queries: [{
      action: 'propose',
      steps: [{ text: 'Implement the small change' }],
      consequential: false,
      reason: 'single local test change',
      reasoning: 'request explicit plan approval',
    }],
  }, undefined, undefined, ctx) as { content: Array<{ text: string }>; details?: Record<string, unknown> };

  assert.match(res.content[0]!.text, /approval pending/i);
  assert.deepEqual(res.details?.pendingInteraction, {
    version: 1,
    interactionId: 'approval-1',
    correlationId: 'approval-correlation-1',
    sessionId: 'session-1',
  });
});

test('plan(clarify) with no interactive host lists the questions to ask inline', async () => {
  // The handler branches on `!ctx`; simulate a host with no ctx.
  const tool = loadTool();
  const res = (await tool.execute('id', { queries: [{ action: 'clarify', questions: [{ prompt: 'Which DB?' }], reasoning: 'exercise headless clarification' }] }, undefined, undefined, undefined)) as { content: Array<{ text: string }> };
  assert.match(res.content[0]!.text, /cannot prompt/);
  assert.match(res.content[0]!.text, /Which DB\?/);
});
