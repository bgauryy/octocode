import { visibleWidth } from '../src/tui/width.js';
import assert from 'node:assert/strict';
import { test } from 'vitest';

import { buildPlanReadModel } from '../src/tools/plan-read-model.js';
import { buildCompactionCard, buildRecoveryCard } from '../src/tools/custom-messages.js';
import { buildOctocodeRenderResult } from '../src/tools/render-helpers.js';
import type { PlanStep } from '../src/tools/planning/plan-types.js';
import type { ToolCallResult } from '../src/types.js';
import { projectPlanStatus } from './helpers/plan-status.js';

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

test('footer plan projection shows progress and the current task without duplicating the checklist', () => {
  const segments = projectPlanStatus(PLAN_MODEL);
  assert.deepEqual(segments.map((segment) => segment.text), [
    'Plan',
    '1 done',
    'running: Unifying state projections',
    '1 active',
    '1 waiting',
    'plan',
  ]);
});

test('compaction and recovery messages share closed, width-perfect component frames', () => {
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
      buildRecoveryCard({
        outcome: 'restored', reason: 'compaction', validated: ['plan'], restored: ['plan'],
        stale: [], corrupt: [], overBudget: [], pendingInteractionIds: [],
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
