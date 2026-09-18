#!/usr/bin/env node
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, unlinkSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildDecisionPacket, validateDecisionPacket, DEFAULT_POLICY } from './decision-contract.mjs';

if (process.argv.slice(2).some(arg => ['--help', '-h'].includes(arg))) {
  console.log('Usage: node scripts/verify-reasoning-loop.mjs [--skip-dryrun]\nValidates the private brief, six public request schemas, APPLY schema, policy, generated packets, and native dry-runs. No API calls.');
  process.exit(0);
}
const skipDryrun = process.argv.includes('--skip-dryrun');
if (process.argv.slice(2).some(arg => arg !== '--skip-dryrun')) throw new Error('Unknown option; use --help.');
const root = new URL('../', import.meta.url);
const launcher = fileURLToPath(new URL('scripts/jev.mjs', root));
const load = name => JSON.parse(readFileSync(new URL(`assets/${name}`, root), 'utf8'));
const schemas = {
  hunch_check: 'hunch.schema.json',
  hypothesis_triage: 'hypothesis-triage.schema.json',
  decision_review: 'decision-review.schema.json',
  reflection_delta: 'reflection-delta.schema.json',
  disputed_inference: 'claim-check.schema.json',
  hallucination_gate: 'hallucination-gate.schema.json'
};
const brief = observations => ({
  observations,
  inferences: ['A semantic relationship may explain the observation.'],
  assumptions: ['The supplied source scope is current.'],
  strongest_counter: 'A different supplied explanation could fit.',
  uncertainty: 'The distinguishing observation is not yet established.',
  falsifier: 'A named contrary observation would weaken the current belief.',
  discriminating_observation: 'Run the supplied check and compare both branches.',
  direct_check: { available: false },
  jev_will_change_action: true
});
const evidence = [{ id: 'E1', kind: 'source-snippet', source: 'fixture:1', scope: 'revision abc', content: 'Warm requests fail and cold requests pass.' }];
const hypotheses = [
  { id: 'H1', statement: 'The cache path is stale.', assumption: 'Warm requests read the cache.', predicts: ['Bypass passes.'], weakenedBy: 'Bypass also fails.' },
  { id: 'H2', statement: 'The caller branch differs.', assumption: 'Warm requests select another branch.', predicts: ['Caller traces differ.'], weakenedBy: 'Caller traces match.' }
];
const next_checks = [
  { id: 'C1', action: 'Run cache bypass.', cost: 'low', expectedOutcomes: [{ observation: 'Bypass passes.', effect: { H1: 'strengthen', H2: 'weaken' } }, { observation: 'Bypass fails.', effect: { H1: 'weaken', H2: 'strengthen' } }] },
  { id: 'C2', action: 'Compare caller traces.', cost: 'medium', expectedOutcomes: [{ observation: 'Traces differ.', effect: { H1: 'weaken', H2: 'strengthen' } }, { observation: 'Traces match.', effect: { H1: 'strengthen', H2: 'weaken' } }] }
];
const inputs = {
  hunch_check: { route: 'hunch_check', model: 'jev-latest', decisionBrief: brief('A warm-only failure was observed.'), state: { mainGoal: 'Find the cause.', goal: 'Decide whether to frame cache hypotheses.', hunch: 'The cache may remain stale after writes.', basis: 'Only warm requests reproduce the failure.' } },
  hypothesis_triage: { route: 'hypothesis_triage', model: 'jev-latest', decisionBrief: brief('E1 records warm-only failure.'), state: { mainGoal: 'Find the cause.', goal: 'Choose a discriminating check.', evidence, hypotheses, next_checks, unknowns: [] } },
  decision_review: { route: 'decision_review', model: 'jev-latest', decisionBrief: brief('The proposed check is expensive.'), state: { mainGoal: 'Avoid a wasted migration.', goal: 'Review the proposal.', proposal: 'Run C1 across the production dataset.', actionCost: 'high', difficultToReverse: true, assumptions: [{ id: 'A1', statement: 'The fixture matches production.' }], risks: [{ id: 'R1', statement: 'The check may not distinguish H1 and H2.' }, { id: 'R2', statement: 'The evidence may be stale.' }] } },
  reflection_delta: { route: 'reflection_delta', model: 'jev-latest', decisionBrief: brief('E7 records that bypass also failed.'), state: { mainGoal: 'Find the cause.', goal: 'Update hypotheses after E7.', priorLead: 'H1', priorProbability: 0.7, hypotheses: hypotheses.map(({ id, statement, predicts }) => ({ id, statement, predicts })), check: { id: 'C1', action: 'Run cache bypass.', expectedOutcomes: ['Pass favors H1.', 'Fail favors H2.'] }, newEvidence: { id: 'E7', source: 'run:7', scope: 'revision abc', content: 'Bypass also failed.' }, unknowns: [] } },
  disputed_inference: { route: 'disputed_inference', model: 'jev-1.13.0', decisionBrief: brief('E1 records the observed ordering.'), state: { mainGoal: 'Write a scoped conclusion.', goal: 'Check one claim.', claim: 'Warm requests fail in revision abc.', current_rationale: 'E1 directly records the failure.', counterclaim: 'The observation may come from another revision.', evidence, evidence_bases: [{ id: 'B1', evidenceIds: ['E1'], description: 'E1 directly records the scoped failure.' }], unknowns: [] } },
  hallucination_gate: { route: 'hallucination_gate', model: 'jev-latest', decisionBrief: brief('E1 is the proposed assertion anchor.'), state: { goal: 'Publish one scoped claim.', claim: 'Warm requests fail in revision abc.', claim_scope: 'revision abc', evidence: evidence.map(({ id, source, scope, content }) => ({ id, source, scope, content })) } }
};

console.log('Phase 1 — schema and policy contracts:');
const decisionBriefSchema = load('decision-brief.schema.json');
assert.ok(decisionBriefSchema.required.includes('observations'));
assert.ok(decisionBriefSchema.required.includes('jev_will_change_action'));
assert.equal(decisionBriefSchema.required.includes('falsifier'), false);
assert.equal(decisionBriefSchema.required.includes('discriminating_observation'), false);
assert.equal(JSON.stringify(decisionBriefSchema).includes('AgentCOT'), false);
for (const [route, name] of Object.entries(schemas)) {
  const schema = load(name);
  assert.equal(schema.type, 'object', `${name}: object schema`);
  assert.deepEqual(schema.required, ['model', 'state', 'questions'], `${name}: exact envelope`);
  assert.match(schema.properties.state.description, /VISIBILITY BOUNDARY/);
  assert.equal(JSON.stringify(schema).includes('AgentCOT'), false, `${name}: stale AgentCOT`);
  const reasoningSchema = schema.$defs.PublicReasoning || schema.$defs.ReasoningSummary;
  assert.ok(reasoningSchema, `${name}: bounded reasoning summary schema`);
  assert.deepEqual(reasoningSchema.required, ['observations', 'uncertainty'], `${name}: minimum auditable reasoning context`);
  assert.match(reasoningSchema.description, /never hidden chain-of-thought|private scratch/i, `${name}: private reasoning boundary`);
  const packet = buildDecisionPacket(inputs[route], DEFAULT_POLICY);
  assert.deepEqual(Object.keys(packet), ['model', 'state', 'questions']);
  assert.equal(validateDecisionPacket(route, packet, DEFAULT_POLICY).valid, true);
  console.log(`  ✓ ${name}`);
}
const hunch = load('hunch.schema.json');
assert.equal(hunch.$defs.PublicReasoning.required.includes('falsifier'), false);
const triage = load('hypothesis-triage.schema.json');
assert.equal(triage.$defs.PublicReasoning.required.includes('falsifier'), false);
assert.ok(triage.properties.state.properties.hypotheses.items.required.includes('predicts'));
assert.ok(triage.properties.state.properties.next_checks.items.required.includes('expectedOutcomes'));
const apply = load('apply-output.schema.json');
for (const field of ['model_used', 'answers', 'net_action', 'blocked', 'block_reasons']) assert.ok(apply.required.includes(field));
for (const name of ['AppliedChoice', 'AppliedNoul', 'AppliedScore']) assert.equal(apply.$defs[name].properties.provisional.const, true);
const runLoopInput = load('run-loop-input.schema.json');
assert.deepEqual(runLoopInput.required, ['route', 'willChangeAction', 'state']);
assert.match(runLoopInput.properties.reasoning.description, /never hidden chain-of-thought|private scratch/i);
assert.equal(DEFAULT_POLICY.softTieGap, 0.15);
assert.equal(DEFAULT_POLICY.maxJevCallsPerCrossroad, 2);
console.log('  ✓ decision-brief.schema.json, run-loop-input.schema.json, apply-output.schema.json, default-policy.json');

if (!skipDryrun) {
  console.log('\nPhase 2 — generated packet native dry-runs:');
  for (const [route, input] of Object.entries(inputs)) {
    const path = join(tmpdir(), `jev-${route}-${process.pid}.json`);
    try {
      writeFileSync(path, JSON.stringify(buildDecisionPacket(input, DEFAULT_POLICY)));
      const result = spawnSync(process.execPath, [launcher, 'evaluate', '--input', path, '--dry-run'], { encoding: 'utf8' });
      assert.equal(result.status, 0, `${route}: ${result.stderr}`);
      assert.deepEqual(Object.keys(JSON.parse(result.stdout)).sort(), ['model', 'questions', 'state']);
      console.log(`  ✓ ${route}`);
    } finally { try { unlinkSync(path); } catch {} }
  }
}
console.log('\nAll reasoning-loop checks passed. No API calls made.');
