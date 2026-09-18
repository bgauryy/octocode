import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  applyResponse,
  buildDecisionPacket,
  routeDecision,
  validateDecisionPacket
} from './decision-contract.mjs';

const root = new URL('../', import.meta.url);
const policy = JSON.parse(readFileSync(new URL('assets/default-policy.json', root), 'utf8'));
const fixtures = JSON.parse(readFileSync(new URL('evals/decision-cases.json', root), 'utf8')).cases;
const brief = {
  observations: 'E1 records a warm-only failure.',
  inferences: ['The cache path may be involved.'],
  assumptions: ['The reproduction uses the same cache layer.'],
  strongest_counter: 'A caller-specific branch could produce the same symptom.',
  uncertainty: 'The bypass behavior has not been observed.',
  falsifier: 'The failure remains when caching is bypassed.',
  discriminating_observation: 'Compare the same request with and without caching.',
  direct_check: { available: false },
  jev_will_change_action: true
};
const triageInput = () => ({
  route: 'hypothesis_triage',
  model: 'jev-1.13.0',
  decisionBrief: structuredClone(brief),
  state: {
    mainGoal: 'Find the regression cause.',
    goal: 'Choose the first discriminating check.',
    evidence: [{ id: 'E1', source: 'test.log:1', scope: 'revision abc', content: 'Warm requests fail; cold requests pass.' }],
    hypotheses: [
      { id: 'H1', statement: 'Invalidation is stale.', assumption: 'The failing path reads the cache.', predicts: ['Bypass returns fresh data.', 'Warm requests retain stale data.'], weakenedBy: 'Bypass still fails.' },
      { id: 'H2', statement: 'The caller branch is wrong.', assumption: 'Warm requests select another caller branch.', predicts: ['Bypass still fails.', 'Caller traces differ.'], weakenedBy: 'Caller traces are identical.' }
    ],
    next_checks: [
      {
        id: 'C1', action: 'Run the request with cache bypassed.', cost: 'low',
        expectedOutcomes: [
          { observation: 'Bypass passes.', effect: { H1: 'strengthen', H2: 'weaken' } },
          { observation: 'Bypass fails.', effect: { H1: 'weaken', H2: 'strengthen' } }
        ]
      },
      {
        id: 'C2', action: 'Compare warm and cold caller traces.', cost: 'medium',
        expectedOutcomes: [
          { observation: 'Caller traces differ.', effect: { H1: 'weaken', H2: 'strengthen' } },
          { observation: 'Caller traces match.', effect: { H1: 'strengthen', H2: 'weaken' } }
        ]
      }
    ],
    unknowns: ['Cache bypass result']
  }
});

test('all planned routing fixtures are executable and frozen', () => {
  assert.equal(fixtures.length, 15);
  for (const fixture of fixtures) assert.deepEqual(routeDecision(fixture.input, policy), fixture.expected, fixture.id);
});

test('builder strips DecisionBrief and emits a valid triage request', () => {
  const packet = buildDecisionPacket(triageInput(), policy);
  assert.deepEqual(Object.keys(packet), ['model', 'state', 'questions']);
  assert.equal(JSON.stringify(packet).includes('decisionBrief'), false);
  assert.equal(packet.state.reasoning.observations, brief.observations);
  assert.deepEqual(Object.keys(packet.questions.hypothesis.criteria), ['H1', 'H2', 'none']);
  assert.deepEqual(Object.keys(packet.questions.next_check.criteria), ['C1', 'C2', 'none']);
  assert.equal(validateDecisionPacket('hypothesis_triage', packet, policy).valid, true);
});

test('generic briefs do not manufacture prediction or falsifier fields', () => {
  const packet = buildDecisionPacket({
    route: 'hunch_check',
    model: 'jev-1.13.0',
    decisionBrief: {
      observations: 'E1 contains a request that needs classification.',
      uncertainty: 'The supplied categories overlap semantically.',
      direct_check: { available: false },
      jev_will_change_action: true
    },
    state: {
      mainGoal: 'Route one bounded research request.',
      goal: 'Decide whether the weak category hunch deserves review.',
      hunch: 'The request may belong to architecture review.',
      basis: 'It asks about boundaries but proposes no implementation.'
    }
  }, policy);
  assert.deepEqual(packet.state.reasoning, {
    observations: 'E1 contains a request that needs classification.',
    uncertainty: 'The supplied categories overlap semantically.'
  });
  assert.equal(Object.hasOwn(packet.state.reasoning, 'prediction'), false);
  assert.equal(Object.hasOwn(packet.state.reasoning, 'falsifier'), false);
});

test('builder skips Jev for direct checks, inert calls, and exhausted crossroads', () => {
  const direct = triageInput(); direct.decisionBrief.direct_check = { available: true, action: 'Read the exact config value.' };
  assert.throws(() => buildDecisionPacket(direct, policy), /direct deterministic check/i);
  const inert = triageInput(); inert.decisionBrief.jev_will_change_action = false;
  assert.throws(() => buildDecisionPacket(inert, policy), /change the next action/i);
  const exhausted = triageInput(); exhausted.jevCallsAtCrossroad = policy.maxJevCallsPerCrossroad;
  assert.throws(() => buildDecisionPacket(exhausted, policy), /call limit/i);
  assert.deepEqual(routeDecision({ willChangeAction: true, jevCallsAtCrossroad: 2 }, policy), { route: 'no_jev', questionTypes: [], policyAction: 'continue_host_research', mayAssert: false });
});

test('testable triage requires precommitted predictions, weakening conditions, and branchable checks', () => {
  const noPrediction = buildDecisionPacket(triageInput(), policy);
  delete noPrediction.state.hypotheses[0].predicts;
  assert.equal(validateDecisionPacket('hypothesis_triage', noPrediction, policy).valid, false);
  const noWeakeningCondition = buildDecisionPacket(triageInput(), policy);
  delete noWeakeningCondition.state.hypotheses[0].weakenedBy;
  assert.equal(validateDecisionPacket('hypothesis_triage', noWeakeningCondition, policy).valid, false);
  const futureResult = buildDecisionPacket(triageInput(), policy);
  futureResult.state.next_checks[0].result = 'Bypass passes.';
  assert.equal(validateDecisionPacket('hypothesis_triage', futureResult, policy).valid, false);
  const nondiscriminating = buildDecisionPacket(triageInput(), policy);
  nondiscriminating.state.next_checks[0].expectedOutcomes = [
    { observation: 'Result exists.', effect: { H1: 'neutral', H2: 'neutral' } },
    { observation: 'Result absent.', effect: { H1: 'neutral', H2: 'neutral' } }
  ];
  assert.equal(validateDecisionPacket('hypothesis_triage', nondiscriminating, policy).valid, false);
});

test('builder creates reflection and decision-review question contracts', () => {
  const reflection = buildDecisionPacket({
    route: 'reflection_delta', model: 'jev-1.13.0', decisionBrief: structuredClone(brief),
    state: {
      mainGoal: 'Find the regression cause.', goal: 'Update hypotheses after E7.', priorLead: 'H1',
      hypotheses: [
        { id: 'H1', statement: 'Invalidation is stale.', predicts: ['Bypass passes.'] },
        { id: 'H2', statement: 'Caller branch is wrong.', predicts: ['Bypass fails.'] }
      ],
      check: { id: 'C1', action: 'Run bypass.', expectedOutcomes: ['pass favors H1', 'fail favors H2'] },
      newEvidence: { id: 'E7', source: 'run:7', scope: 'revision abc', content: 'Bypass also failed.' },
      unknowns: []
    }
  }, policy);
  assert.deepEqual(Object.values(reflection.questions).map(q => q.type), ['choice', 'choice', 'noul']);
  assert.equal(validateDecisionPacket('reflection_delta', reflection, policy).valid, true);

  const review = buildDecisionPacket({
    route: 'decision_review', model: 'jev-1.13.0', decisionBrief: structuredClone(brief),
    state: {
      mainGoal: 'Avoid an expensive wrong investigation.', goal: 'Review C1 before execution.',
      proposal: 'Investigate H1 with C1.', actionCost: 'high', difficultToReverse: true,
      assumptions: [{ id: 'A1', statement: 'The reproduction uses the same cache layer.' }],
      risks: [
        { id: 'R1', statement: 'C1 does not separate H1 and H2.' },
        { id: 'R2', statement: 'Evidence is from another revision.' }
      ]
    }
  }, policy);
  assert.deepEqual(Object.values(review.questions).map(q => q.type), ['noul', 'choice', 'noul']);
  assert.equal(validateDecisionPacket('decision_review', review, policy).valid, true);
});

test('hallucination gate blocks empty evidence and incompatible scope before Jev', () => {
  const base = {
    route: 'hallucination_gate', model: 'jev-1.13.0', decisionBrief: structuredClone(brief),
    state: { goal: 'Publish a claim.', claim: 'The guard works in production.', claim_scope: 'production', evidence: [] }
  };
  assert.throws(() => buildDecisionPacket(base, policy), /without evidence/i);
  base.state.evidence = [{ id: 'E1', source: 'staging.log:1', scope: 'staging', content: 'The guard passed.' }];
  assert.throws(() => buildDecisionPacket(base, policy), /scope/i);
});

test('applyResponse computes policy reads and keeps every answer provisional', () => {
  const request = buildDecisionPacket(triageInput(), policy);
  const response = {
    model: 'jev-1.13.0', usage: { input_tokens: 100, output_tokens: 20 }, answers: {
      hypothesis: { type: 'choice', choice: 'H1', probabilities: { H1: 0.7, H2: 0.2, none: 0.1 }, confidence: 0.6 },
      next_check: { type: 'choice', choice: 'C1', probabilities: { C1: 0.52, C2: 0.38, none: 0.1 }, confidence: 0.3 }
    }
  };
  const applied = applyResponse(request, response, {
    hypothesis: 'Run C1 against src/cache.ts and retain H2.',
    next_check: 'Run C1 with cache bypass and record both branches.'
  }, 'Run C1 with cache bypass and record the observation.', policy);
  assert.equal(applied.answers[0].confidence_read, 'sharp');
  assert.equal(applied.answers[1].confidence_read, 'soft-tie');
  assert.ok(applied.answers.every(answer => answer.provisional === true));
  assert.equal(applied.answers[1].policy_preference, 'C1');
  assert.equal(applied.blocked, false);
  assert.equal(applied.net_action.includes('C1'), true);
});

test('applyResponse blocks ambiguous Noul advice', () => {
  const request = buildDecisionPacket({
    route: 'hunch_check', model: 'jev-1.13.0', decisionBrief: structuredClone(brief),
    state: { mainGoal: 'Choose a research direction.', goal: 'Decide whether to promote a hunch.', hunch: 'The cache path may remain stale.', basis: 'Only warm requests fail.' }
  }, policy);
  const response = { model: 'jev-1.13.0', answers: { worth_pursuing: { type: 'noul', noul: 0.5 } }, usage: { input_tokens: 1, output_tokens: 1 } };
  const applied = applyResponse(request, response, { worth_pursuing: 'Retrieve cache bypass evidence before promotion.' }, 'Retrieve cache bypass evidence before promoting the hunch.', policy);
  assert.equal(applied.blocked, true);
  assert.match(applied.block_reasons[0], /ambiguous/i);
});

test('applyResponse blocks none and low-grounding outcomes', () => {
  const request = buildDecisionPacket(triageInput(), policy);
  const noneResponse = {
    model: 'jev-1.13.0', answers: {
      hypothesis: { type: 'choice', choice: 'none', probabilities: { H1: 0.1, H2: 0.2, none: 0.7 }, confidence: 0.6 },
      next_check: { type: 'choice', choice: 'C1', probabilities: { C1: 0.7, C2: 0.2, none: 0.1 }, confidence: 0.6 }
    }, usage: { input_tokens: 1, output_tokens: 1 }
  };
  const applied = applyResponse(request, noneResponse, { hypothesis: 'Reframe hypotheses before any check.', next_check: 'Do not run C1 until hypotheses are reframed.' }, 'Reframe the hypothesis set before running a check.', policy);
  assert.equal(applied.blocked, true);
  assert.match(applied.block_reasons[0], /none/i);
});
