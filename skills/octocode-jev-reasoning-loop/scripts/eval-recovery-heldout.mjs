#!/usr/bin/env node
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { prepareCompactRun } from './decision-contract.mjs';
import { runLoop } from './run-loop.mjs';

const suite = JSON.parse(readFileSync(new URL('../evals/recovery-heldout.json', import.meta.url), 'utf8'));
const argv = process.argv.slice(2);
if (argv.includes('--help') || argv.includes('-h')) {
  console.log('Usage: node scripts/eval-recovery-heldout.mjs [--live --output DIR]\nWithout --live, validates frozen packets without API calls. --live runs triage, delta, and claim-check for each case and records recovery, false recovery, unsupported claims, calls, and tokens.');
  process.exit(0);
}
const live = argv.includes('--live');
const outputAt = argv.indexOf('--output');
if (outputAt >= 0 && !argv[outputAt + 1]) throw new Error('--output requires a directory.');
const output = resolve(outputAt >= 0 ? argv[outputAt + 1] : '.octocode/octocode-jev-reasoning-loop/benchmark/recovery-heldout-v1');
const unexpected = argv.filter((arg, index) => !['--live', '--output'].includes(arg) && index !== outputAt + 1);
if (unexpected.length) throw new Error(`Unknown options: ${unexpected.join(', ')}`);

function checksFor(item) {
  return [item.check, {
    id: 'C2', action: item.alternateCheck, cost: 'medium', expectedOutcomes: [
      { observation: 'The alternative check favors H1.', effect: { H1: 'strengthen', H2: 'weaken' } },
      { observation: 'The alternative check favors H2.', effect: { H1: 'weaken', H2: 'strengthen' } }
    ]
  }];
}

function triageInput(item) {
  return {
    route: 'hypothesis_triage', model: suite.model, willChangeAction: true,
    state: {
      mainGoal: item.goal, goal: 'Select a provisional lead and discriminating check.', scope: item.scope,
      evidence: [{ id: 'E1', source: `heldout:${item.id}:initial`, scope: item.scope, content: item.initialEvidence }],
      hypotheses: item.hypotheses, next_checks: checksFor(item), unknowns: ['Result of C1']
    }
  };
}

function deltaInput(item, priorLead, priorProbability) {
  return {
    route: 'reflection_delta', model: suite.model, willChangeAction: true,
    state: {
      mainGoal: item.goal, goal: 'Update or abandon the prior lead after C1.', priorLead, priorProbability,
      hypotheses: item.hypotheses.map(({ id, statement, predicts }) => ({ id, statement, predicts })),
      check: { id: item.check.id, action: item.check.action, expectedOutcomes: item.check.expectedOutcomes.map(outcome => outcome.observation) },
      newEvidence: { id: 'E2', source: `heldout:${item.id}:observed`, scope: item.scope, content: item.newEvidence },
      unknowns: []
    }
  };
}

function claimInput(item, lead) {
  const chosen = item.hypotheses.find(hypothesis => hypothesis.id === lead);
  const other = item.hypotheses.find(hypothesis => hypothesis.id !== lead);
  return {
    route: 'disputed_inference', model: suite.model, willChangeAction: true, evidenceFresh: true,
    state: {
      mainGoal: item.goal, goal: 'Check the final bounded hypothesis before assertion.',
      claim: chosen.statement, current_rationale: 'The initial and newly observed evidence are supplied below.',
      counterclaim: other.statement,
      evidence: [
        { id: 'E1', kind: 'observation', source: `heldout:${item.id}:initial`, scope: item.scope, content: item.initialEvidence },
        { id: 'E2', kind: 'observation', source: `heldout:${item.id}:observed`, scope: item.scope, content: item.newEvidence }
      ],
      evidence_bases: [
        { id: 'B1', evidenceIds: ['E1'], description: 'Initial observation only.' },
        { id: 'B2', evidenceIds: ['E1', 'E2'], description: 'Initial and discriminating observations together.' }
      ],
      unknowns: []
    }
  };
}

function validateFrozenPackets() {
  let packets = 0;
  for (const item of suite.cases) {
    const triage = prepareCompactRun(triageInput(item));
    if (triage.status !== 'ready') throw new Error(`${item.id}: triage ${triage.status}`);
    const delta = prepareCompactRun(deltaInput(item, 'H1', 0.6));
    if (delta.status !== 'ready') throw new Error(`${item.id}: delta ${delta.status}`);
    const claim = prepareCompactRun(claimInput(item, item.truth));
    if (claim.status !== 'ready') throw new Error(`${item.id}: claim ${claim.status}`);
    packets += 3;
  }
  return { suiteVersion: suite.version, frozen: suite.frozen, mode: 'self-test', cases: suite.cases.length, packets, passed: true };
}

if (!live) {
  console.log(JSON.stringify(validateFrozenPackets()));
} else {
  mkdirSync(output, { recursive: true });
  const rows = [];
  let calls = 0, inputTokens = 0, outputTokens = 0;
  for (const item of suite.cases) {
    const caseDir = join(output, item.id);
    const triage = runLoop(triageInput(item), { output: join(caseDir, 'triage'), timeoutMs: '20000', retries: '0' });
    calls += triage.summary.metrics.api_calls;
    inputTokens += triage.summary.metrics.input_tokens;
    outputTokens += triage.summary.metrics.output_tokens;
    const initialLead = triage.summary.decisions?.hypothesis?.selected;
    const initialProbability = triage.summary.decisions?.hypothesis?.probability;
    if (!['H1', 'H2'].includes(initialLead)) {
      rows.push({ id: item.id, kind: item.kind, truth: item.truth, initialLead, status: 'no_initial_lead', calls: 1 });
      continue;
    }
    const delta = runLoop(deltaInput(item, initialLead, initialProbability), { output: join(caseDir, 'delta'), timeoutMs: '20000', retries: '0' });
    calls += delta.summary.metrics.api_calls;
    inputTokens += delta.summary.metrics.input_tokens;
    outputTokens += delta.summary.metrics.output_tokens;
    const finalLead = delta.summary.decisions?.updated_lead?.selected;
    const effect = delta.summary.decisions?.effect_on_prior_lead?.selected;
    const reframe = delta.summary.decisions?.reframe_needed?.value;
    let claim;
    if (['H1', 'H2'].includes(finalLead)) {
      claim = runLoop(claimInput(item, finalLead), { output: join(caseDir, 'claim'), timeoutMs: '20000', retries: '0' });
      calls += claim.summary.metrics.api_calls;
      inputTokens += claim.summary.metrics.input_tokens;
      outputTokens += claim.summary.metrics.output_tokens;
    }
    const wrongLean = item.kind === 'wrong-lean' && initialLead !== item.truth;
    const recovered = wrongLean && (finalLead !== initialLead || reframe >= 0.6);
    const falseRecovery = item.kind === 'control' && initialLead === item.truth && (finalLead !== initialLead || reframe >= 0.6);
    const claimStatus = claim?.summary.decisions?.claim_status?.selected;
    const unsupportedClaim = finalLead && finalLead !== item.truth && claim?.summary.status === 'applied' && claimStatus === 'supported';
    rows.push({
      id: item.id, kind: item.kind, truth: item.truth, initialLead, initialProbability,
      effect, finalLead, reframe, wrongLean, recovered, falseRecovery,
      claimStatus, claimGate: claim?.summary.status, unsupportedClaim: Boolean(unsupportedClaim),
      calls: 2 + (claim ? 1 : 0)
    });
  }
  const wrongPopulation = rows.filter(row => row.wrongLean);
  const controls = rows.filter(row => row.kind === 'control' && row.initialLead === row.truth);
  const wrongLeanRecoveryRate = wrongPopulation.length ? wrongPopulation.filter(row => row.recovered).length / wrongPopulation.length : null;
  const falseRecoveryRate = controls.length ? controls.filter(row => row.falseRecovery).length / controls.length : null;
  const claimAttempts = rows.filter(row => row.claimStatus);
  const unsupportedClaimRate = claimAttempts.length ? claimAttempts.filter(row => row.unsupportedClaim).length / claimAttempts.length : null;
  const sensorValid = wrongPopulation.length >= 2 && controls.length >= 1;
  const guardsPass = sensorValid && wrongLeanRecoveryRate >= suite.thresholds.wrongLeanRecoveryMinimum && falseRecoveryRate <= suite.thresholds.falseRecoveryMaximum && unsupportedClaimRate <= suite.thresholds.unsupportedClaimMaximum && rows.every(row => row.calls <= suite.thresholds.maxCallsPerCase);
  const report = {
    suiteVersion: suite.version, frozen: suite.frozen, model: suite.model, cases: rows,
    metrics: {
      wrong_lean_population: wrongPopulation.length,
      wrong_lean_recovery_rate: wrongLeanRecoveryRate,
      control_population: controls.length,
      false_recovery_rate: falseRecoveryRate,
      claim_attempts: claimAttempts.length,
      unsupported_claim_rate: unsupportedClaimRate,
      api_calls: calls,
      input_tokens: inputTokens,
      output_tokens: outputTokens
    },
    sensorValid,
    guardVerdict: guardsPass ? 'PASS' : sensorValid ? 'FAIL' : 'INVALID',
    limitation: 'Absolute held-out characterization only; no matched host-only baseline, so this does not prove comparative efficacy.'
  };
  writeFileSync(join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report));
  if (!guardsPass) process.exitCode = sensorValid ? 1 : 2;
}
