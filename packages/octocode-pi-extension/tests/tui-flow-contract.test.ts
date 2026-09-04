import assert from 'node:assert/strict';
import { test } from 'vitest';
import { renderFooterView } from '../src/tui/footer-view.js';
import { planPanelModelLines } from '../src/tools/plan-tool.js';
import { buildPlanReadModel } from '../src/tools/plan-read-model.js';
import { formatAwarenessPanel } from '../src/tools/awareness-status.js';
import { buildCompactionCard, buildHandoffCard } from '../src/tools/custom-messages.js';
import { buildOctocodeRenderResult, visibleWidth } from '../src/tools/render-helpers.js';
import type { PlanStep } from '../src/tools/active-plan.js';
import type { ToolCallResult } from '../src/types.js';
import { buildPlanFooterSegments } from '../src/extension-ui.js';

const PLAN: PlanStep[] = [
  { id: 'one', text: 'Inventory renderers', status: 'done' },
  { id: 'two', text: 'Unify state projections', activeForm: 'Unifying state projections', status: 'doing' },
  { id: 'three', text: 'Run visual checks', status: 'todo', dependsOnStepIds: ['two'] },
];
const PLAN_MODEL = buildPlanReadModel({
  steps: PLAN,
  review: { phase: 'executing', branchSnapshotId: 'tui-test', generation: 0, decisions: [], blockingQuestions: [], comments: [] },
  coordination: { mode: 'local', sourcePlanKey: 'tui-test', coordinationWorkspace: '' },
});

function assertWidthSafe(lines: readonly string[], width: number): void {
  for (const line of lines) assert.ok(visibleWidth(line) <= width, `${width}: ${line}`);
}

test('plan, task, agent, footer, and Awareness projections stay complete and width-safe', () => {
  for (const width of [24, 40, 80, 120]) {
    const plan = planPanelModelLines(PLAN_MODEL, undefined, width);
    const awareness = formatAwarenessPanel({
      activePlans: 1,
      readyTasks: 2,
      inProgressTasks: 1,
      verifyTasks: 1,
      lockCount: 1,
      workCount: 2,
      agentCount: 2,
      messageCount: 1,
      unreadInbox: 1,
      lastInbound: { from: 'reviewer', preview: 'ready for verification' },
      taskActivities: [
        { taskId: 'task-running', title: 'Implement footer', state: 'doing', agentId: 'builder' },
        { taskId: 'task-ready', title: 'Review borders', state: 'ready' },
      ],
    }, undefined, width);
    const footer = renderFooterView({
      segments: [
        { text: 'main (3 changed)' }, { text: 'model gpt-5.6' },
        { text: 'context 152k/200k · 76%' }, { text: 'agents 2 (1 live)' },
      ],
    }, { width });

    assertWidthSafe([...plan, ...awareness, ...footer], width);
    if (width >= 80) {
      const body = [...plan, ...awareness, ...footer].join('\n');
      assert.match(body, /Unifying state projections/);
      assert.match(body, /gpt-5\.6/);
      if (width >= 120) assert.match(body, /verify-debt 1/);
    }
  }
});

test('footer plan projection shows progress and the current task without duplicating the checklist', () => {
  const segments = buildPlanFooterSegments(PLAN_MODEL);
  assert.deepEqual(segments.map((segment) => segment.text), [
    'plan 1/3',
    'task 2 Unifying state projections',
  ]);
});

test('compaction and Awareness messages share closed, width-perfect component frames', () => {
  for (const width of [24, 48, 96]) {
    const cards = [
      buildCompactionCard({
        label: 'entry-42',
        reason: 'threshold',
        tokensBefore: 180_000,
        fromExtension: true,
        readFiles: ['src/a.ts'],
        modifiedFiles: ['src/b.ts'],
        summary: 'Preserve the active task and plan references.',
        activePlan: { total: 3, done: 1, running: 'Unify state projections' },
      }, true, undefined, width),
      buildHandoffCard({
        label: 'handoff-1',
        from: 'builder',
        to: 'reviewer',
        goal: 'Verify all TUI permutations',
        status: 'in-progress',
        notes: ['Borders checked'],
        artifacts: ['docs/TUI_PERMUTATION_CONTRACT.html'],
      }, true, undefined, width),
    ];
    for (const lines of cards) {
      assert.ok(lines[0]?.startsWith('╭'));
      assert.ok(lines.at(-1)?.startsWith('╰'));
      assertWidthSafe(lines, width);
    }
  }
});

test('tool results render run policy once and retain every per-query receipt', () => {
  const result: ToolCallResult = {
    content: [{ type: 'text', text: '2 queries succeeded · parallel.' }],
    details: {
      queryRunType: 'parallel',
      results: [
        { index: 0, status: 'success', reasoning: 'inspect first', summary: 'first loaded' },
        { index: 1, status: 'failed', reasoning: 'inspect second', summary: 'second failed' },
      ],
    },
  };
  for (const width of [24, 48, 96]) {
    const lines = buildOctocodeRenderResult('readMedia', result, { expanded: false }).render(width);
    assert.equal(lines.length, 3);
    if (width >= 48) assert.match(lines[0]!, /2 queries.*parallel/);
    assert.match(lines[1]!, /\[0\]/);
    assert.match(lines[2]!, /\[1\]/);
    assertWidthSafe(lines, width);
  }
});
