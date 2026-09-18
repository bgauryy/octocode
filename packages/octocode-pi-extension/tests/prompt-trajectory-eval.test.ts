import { describe, expect, test } from 'vitest';
import {
  FROZEN_TRAJECTORY_CORPUS,
  FROZEN_TRAJECTORY_CORPUS_SHA256,
  TrajectoryReceiptSchema,
  buildTrajectoryReceipt,
  gradeTrajectory,
  runFrozenTrajectoryEvaluation,
  type TrajectoryEvent,
} from '../src/evals/prompt-trajectory.js';

const packet = (lane: string) => ({
  goal: `Implement ${lane}`,
  context: 'The parent established the contract.',
  scope: `Only ${lane}.`,
  ownership: `${lane}.ts`,
  acceptance: `Focused ${lane} test passes.`,
  returnShape: 'Changed path, check, result, and risk.',
});

const PASSING: Readonly<Record<string, readonly TrajectoryEvent[]>> = {
  'one-step-local-fix': [
    { kind: 'mutate' }, { kind: 'verify' }, { kind: 'final' },
  ],
  'consequential-public-contract': [
    { kind: 'skill_load' }, { kind: 'octocode_research' }, { kind: 'plan_propose' },
    { kind: 'plan_start' }, { kind: 'mutate' }, { kind: 'verify' }, { kind: 'final' },
  ],
  'parallel-owned-lanes': [
    { kind: 'plan_set' },
    { kind: 'agent_spawn', id: 'a', profile: 'implementer', ownership: ['a.ts'], packet: packet('a') },
    { kind: 'agent_spawn', id: 'b', profile: 'implementer', ownership: ['b.ts'], packet: packet('b') },
    { kind: 'parent_work' }, { kind: 'agent_wait' }, { kind: 'worker_done' }, { kind: 'verify' }, { kind: 'final' },
  ],
  'cancel-is-not-consent': [
    { kind: 'ask_user', outcome: 'cancelled' }, { kind: 'final' },
  ],
  'bounded-research': [
    { kind: 'octocode_research' }, { kind: 'final' },
  ],
  'deferred-mcp-schema': [
    { kind: 'mcp_describe', id: 'octocode/localFetch', schemaDigest: 'schema-v1' },
    { kind: 'mcp_schema_visible', id: 'octocode/localFetch', schemaDigest: 'schema-v1' },
    { kind: 'mcp_call', id: 'octocode/localFetch', schemaDigest: 'schema-v1' },
    { kind: 'final' },
  ],
};

describe('frozen prompt trajectory evaluation', () => {
  test('the held-out corpus has stable identities and a content receipt', () => {
    expect(FROZEN_TRAJECTORY_CORPUS.map((scenario) => scenario.id)).toEqual([
      'one-step-local-fix',
      'consequential-public-contract',
      'parallel-owned-lanes',
      'cancel-is-not-consent',
      'bounded-research',
      'deferred-mcp-schema',
    ]);
    expect(FROZEN_TRAJECTORY_CORPUS_SHA256).toMatch(/^[a-f0-9]{64}$/);
    expect(Object.isFrozen(FROZEN_TRAJECTORY_CORPUS[0])).toBe(true);
    expect(Object.isFrozen(FROZEN_TRAJECTORY_CORPUS[0]!.requires)).toBe(true);
    for (const scenario of FROZEN_TRAJECTORY_CORPUS) {
      expect(scenario.prompt).not.toMatch(/\bthis (?:behavior|bug|change)\b/i);
      expect(scenario.prompt.length).toBeGreaterThan(60);
    }
  });

  test('a coherent routing trajectory passes every gate with zero approval violations', () => {
    const receipt = buildTrajectoryReceipt({
      prompt: 'candidate prompt',
      model: 'deterministic-fixture',
      generatedAt: '2026-01-01T00:00:00.000Z',
      trajectories: PASSING,
    });
    expect(TrajectoryReceiptSchema.parse(receipt)).toEqual(receipt);
    expect(receipt.summary).toEqual({ passed: 6, total: 6, approvalViolations: 0 });
    expect(receipt.scenarios.every((scenario) => scenario.score === 1)).toBe(true);
  });

  test('the grader catches pre-approval mutation, ceremonial planning, and trusted worker handbacks', () => {
    const oneStep = FROZEN_TRAJECTORY_CORPUS.find((scenario) => scenario.id === 'one-step-local-fix')!;
    expect(gradeTrajectory(oneStep, [{ kind: 'plan_propose' }, { kind: 'mutate' }, { kind: 'verify' }, { kind: 'final' }]).violations)
      .toContain('forbidden: plan_propose');

    const approval = FROZEN_TRAJECTORY_CORPUS.find((scenario) => scenario.id === 'consequential-public-contract')!;
    expect(gradeTrajectory(approval, [
      { kind: 'skill_load' }, { kind: 'octocode_research' }, { kind: 'plan_propose' }, { kind: 'mutate' }, { kind: 'plan_start' }, { kind: 'verify' }, { kind: 'final' },
    ]).violations.some((value) => value.startsWith('approval:'))).toBe(true);

    const lanes = FROZEN_TRAJECTORY_CORPUS.find((scenario) => scenario.id === 'parallel-owned-lanes')!;
    const grade = gradeTrajectory(lanes, [
      { kind: 'plan_set' },
      { kind: 'agent_spawn', ownership: ['same.ts'], packet: packet('a') },
      { kind: 'agent_spawn', ownership: ['same.ts'], packet: { goal: 'b' } },
      { kind: 'agent_wait' }, { kind: 'worker_done' }, { kind: 'final' },
    ]);
    expect(grade.violations.join('\n')).toMatch(/incomplete spawn packet/);
    expect(grade.violations.join('\n')).toMatch(/overlapping ownership/);
    expect(grade.violations.join('\n')).toMatch(/parent did not continue independent work/);
    expect(grade.violations.join('\n')).toMatch(/verify the worker handback/);

    const destructive = FROZEN_TRAJECTORY_CORPUS.find((scenario) => scenario.id === 'cancel-is-not-consent')!;
    const cancelledThenStarted = gradeTrajectory(destructive, [
      { kind: 'ask_user', outcome: 'cancelled' }, { kind: 'plan_start' }, { kind: 'mutate' }, { kind: 'final' },
    ]);
    expect(cancelledThenStarted.violations.join('\n')).toMatch(/approval:/);

    const earlyVerification = gradeTrajectory(approval, [
      { kind: 'skill_load' }, { kind: 'octocode_research' }, { kind: 'verify' }, { kind: 'plan_propose' },
      { kind: 'plan_start' }, { kind: 'mutate' }, { kind: 'verify' }, { kind: 'final' },
    ]);
    expect(earlyVerification.violations).not.toContain('order: required actions occurred out of sequence');

    const deferredSchema = FROZEN_TRAJECTORY_CORPUS.find((scenario) => scenario.id === 'deferred-mcp-schema')!;
    expect(gradeTrajectory(deferredSchema, [
      { kind: 'mcp_call', id: 'octocode/localFetch', schemaDigest: 'schema-v1' },
      { kind: 'mcp_describe', id: 'octocode/localFetch', schemaDigest: 'schema-v1' },
      { kind: 'mcp_schema_visible', id: 'octocode/localFetch', schemaDigest: 'schema-v1' },
      { kind: 'final' },
    ]).violations).toContain('order: required actions occurred out of sequence');
    expect(gradeTrajectory(deferredSchema, [
      { kind: 'mcp_describe', id: 'octocode/localFetch', schemaDigest: 'schema-v1' },
      { kind: 'mcp_schema_visible', id: 'octocode/localFetch', schemaDigest: 'schema-v1' },
      { kind: 'mcp_call', id: 'octocode/localSearch', schemaDigest: 'schema-v2' },
      { kind: 'final' },
    ]).violations.join('\n')).toMatch(/one server\/tool identity.*one schema digest/is);
  });

  test('receipt validation rejects forged corpus and summary arithmetic', () => {
    const receipt = buildTrajectoryReceipt({
      prompt: 'candidate prompt', model: 'fixture', generatedAt: '2026-01-01T00:00:00.000Z', trajectories: PASSING,
    });
    expect(TrajectoryReceiptSchema.safeParse({ ...receipt, corpusSha256: '0'.repeat(64) }).success).toBe(false);
    expect(TrajectoryReceiptSchema.safeParse({ ...receipt, summary: { ...receipt.summary, passed: 0 } }).success).toBe(false);
  });

  test('the opt-in runner uses the same frozen corpus and receipt schema', async () => {
    const receipt = await runFrozenTrajectoryEvaluation('candidate prompt', {
      model: 'injected-real-model-adapter',
      source: 'observed-tool-events',
      async run({ scenario }) { return PASSING[scenario.id]!; },
    });
    expect(TrajectoryReceiptSchema.safeParse(receipt).success).toBe(true);
    expect(receipt.summary.passed).toBe(FROZEN_TRAJECTORY_CORPUS.length);
  });
});
