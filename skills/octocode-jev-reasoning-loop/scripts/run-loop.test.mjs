import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { runLoop } from './run-loop.mjs';

const triageInput = () => ({
  route: 'hypothesis_triage',
  model: 'jev-1.13.0',
  willChangeAction: true,
  state: {
    mainGoal: 'Find the regression cause.',
    goal: 'Choose the first discriminating check.',
    scope: 'revision abc',
    evidence: [{ id: 'E1', source: 'test.log:1', scope: 'revision abc', content: 'Warm requests fail; cold requests pass.' }],
    hypotheses: [
      { id: 'H1', statement: 'Invalidation is stale.', assumption: 'The failing path reads the cache.', predicts: ['Bypass passes.'], weakenedBy: 'Bypass fails.' },
      { id: 'H2', statement: 'Caller branch is wrong.', assumption: 'Warm requests choose another branch.', predicts: ['Traces differ.'], weakenedBy: 'Traces match.' }
    ],
    next_checks: [
      { id: 'C1', action: 'Run the request with cache bypassed.', cost: 'low', expectedOutcomes: [
        { observation: 'Bypass passes.', effect: { H1: 'strengthen', H2: 'weaken' } },
        { observation: 'Bypass fails.', effect: { H1: 'weaken', H2: 'strengthen' } }
      ] },
      { id: 'C2', action: 'Compare warm and cold caller traces.', cost: 'medium', expectedOutcomes: [
        { observation: 'Traces differ.', effect: { H1: 'weaken', H2: 'strengthen' } },
        { observation: 'Traces match.', effect: { H1: 'strengthen', H2: 'weaken' } }
      ] }
    ],
    unknowns: []
  }
});

const triageResponse = {
  model: 'jev-1.13.0',
  answers: {
    hypothesis: { type: 'choice', choice: 'H1', probabilities: { H1: 0.7, H2: 0.2, none: 0.1 }, confidence: 0.6 },
    next_check: { type: 'choice', choice: 'C1', probabilities: { C1: 0.8, C2: 0.15, none: 0.05 }, confidence: 0.8 }
  },
  usage: { input_tokens: 100, output_tokens: 20 }
};

function withTemp(run) {
  const dir = mkdtempSync(join(tmpdir(), 'jev-run-loop-'));
  try { return run(dir); }
  finally { rmSync(dir, { recursive: true, force: true }); }
}

test('one-command runner evaluates a recorded response and generates provisional APPLY', () => withTemp(dir => {
  const response = join(dir, 'response.json');
  writeFileSync(response, JSON.stringify(triageResponse));
  const result = runLoop(triageInput(), { output: join(dir, 'run'), response });
  assert.equal(result.exitCode, 0);
  assert.equal(result.summary.status, 'applied');
  assert.equal(result.summary.metrics.api_calls, 0);
  assert.equal(result.summary.netAction, 'Run the request with cache bypassed.');
  assert.equal(result.summary.decisions.hypothesis.selected, 'H1');
  for (const path of Object.values(result.summary.artifacts)) assert.equal(existsSync(path), true);
}));

test('one-command runner fails closed with an observed-facts claim narrowing', () => withTemp(dir => {
  const input = {
    route: 'disputed_inference', model: 'jev-1.13.0', willChangeAction: true, evidenceFresh: true,
    state: {
      mainGoal: 'Write one scoped conclusion.', goal: 'Check the claim.',
      claim: 'The guard prevents every parser failure.', current_rationale: 'E1 records one guard.',
      counterclaim: 'Other parser failures remain possible.',
      evidence: [{ id: 'E1', kind: 'source-snippet', source: 'src/check.rs:10', scope: 'revision abc', content: 'Oversized input returns before parse.' }],
      evidence_bases: [{ id: 'B1', evidenceIds: ['E1'], description: 'One guard observation.' }], unknowns: []
    }
  };
  const responseValue = {
    model: 'jev-1.13.0', answers: {
      claim_status: { type: 'choice', choice: 'insufficient', probabilities: { supported: 0.4, contradicted: 0.1, insufficient: 0.45, conflicting: 0.05 }, confidence: 0.2 },
      decisive_basis: { type: 'choice', choice: 'B1', probabilities: { B1: 0.9, none: 0.1 }, confidence: 0.8 }
    }, usage: { input_tokens: 80, output_tokens: 20 }
  };
  const response = join(dir, 'response.json');
  writeFileSync(response, JSON.stringify(responseValue));
  const result = runLoop(input, { output: join(dir, 'run'), response });
  assert.equal(result.exitCode, 4);
  assert.equal(result.summary.status, 'blocked');
  assert.equal(result.summary.narrowing.strategy, 'report_observed_facts');
  assert.match(result.summary.narrowing.claim, /E1 records/);
  assert.equal(Object.hasOwn(result.summary.artifacts, 'apply'), false);
}));

test('runner help presents the compact single-entry workflow', () => {
  const result = spawnSync(process.execPath, [new URL('./run-loop.mjs', import.meta.url).pathname, '--help'], { encoding: 'utf8' });
  assert.equal(result.status, 0);
  assert.match(result.stdout, /One entry point/);
  assert.match(result.stdout, /replaces DecisionBrief and action-map/);
});

test('claim CLI builds the same evidence packet without authored IDs or copied source', () => withTemp(dir => {
  writeFileSync(join(dir, 'café.txt'), 'Observed café behavior.\nCounterevidence remains bounded.');
  const response = join(dir, 'recorded.json');
  writeFileSync(response, JSON.stringify({ model: 'jev-1.13.0', answers: {
    grounded: { type: 'noul', noul: 0.95 },
    evidence_anchor: { type: 'choice', choice: 'E1', probabilities: { E1: 0.95, E2: 0.04, none: 0.01 }, confidence: 0.9 },
    scope_matches: { type: 'noul', noul: 0.95 }
  }, usage: { input_tokens: 100, output_tokens: 20 } }));
  const output = join(dir, 'short');
  const result = spawnSync(process.execPath, [new URL('./run-loop.mjs', import.meta.url).pathname,
    '--claim', 'The café behavior is supported.', '--scope', 'fixture',
    '--evidence', 'café.txt:1', '--evidence', 'café.txt:2', '--model', 'jev-1.13.0',
    '--response', response, '--output', output], { cwd: dir, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  const explicit = runLoop({ route: 'hallucination_gate', model: 'jev-1.13.0', willChangeAction: true,
    state: { goal: 'Decide whether to state or qualify the supplied claim.', claim: 'The café behavior is supported.', claim_scope: 'fixture',
      evidence: [1, 2].map((line, index) => ({ id: `E${index + 1}`, scope: 'fixture', contentRef: { path: 'café.txt', lines: String(line) } })) }
  }, { rootDir: dir, response, output: join(dir, 'explicit') });
  assert.deepEqual(JSON.parse(readFileSync(join(output, 'request.json'))), JSON.parse(readFileSync(explicit.summary.artifacts.request)));
  assert.equal(JSON.parse(result.stdout).metrics.api_calls, 0);
}));

test('reference savings compare equivalent UTF-8 packets and preserve negative savings', () => withTemp(dir => {
  writeFileSync(join(dir, 'large.txt'), 'é\n' + 'Unselected tail'.repeat(1000));
  const input = triageInput();
  input.state.evidence = [{ id: 'E1', scope: 'revision abc', contentRef: { path: 'large.txt', lines: '1' } }];
  const response = join(dir, 'response.json');
  writeFileSync(response, JSON.stringify(triageResponse));
  const { summary } = runLoop(input, { rootDir: dir, response, output: join(dir, 'run') });
  const inline = structuredClone(input);
  inline.state.evidence = [{ id: 'E1', scope: 'revision abc', content: 'é', source: 'large.txt:L1-L1' }];
  const metrics = summary.metrics.content_ref;
  assert.equal(metrics.reference_input_bytes, Buffer.byteLength(JSON.stringify(input)));
  assert.equal(metrics.inline_equivalent_input_bytes, Buffer.byteLength(JSON.stringify(inline)));
  assert.equal(metrics.input_bytes_avoided, Buffer.byteLength(JSON.stringify(inline)) - Buffer.byteLength(JSON.stringify(input)));
  assert.ok(metrics.input_bytes_avoided < 0);
  assert.equal(Object.hasOwn(metrics, 'host_bytes_saved'), false);
}));

test('claim CLI rejects conflicting input, missing scope, and escaping evidence before evaluation', () => withTemp(dir => {
  const script = new URL('./run-loop.mjs', import.meta.url).pathname;
  for (const [args, error] of [
    [['--input', 'never-read.json', '--claim', 'Some claim.'], /cannot combine/],
    [['--claim', 'Some claim.', '--evidence', 'source.txt:1'], /scope/],
    [['--claim', 'Some claim.', '--scope', 'fixture', '--evidence', '../outside.txt:1'], /sandbox/]
  ]) {
    const result = spawnSync(process.execPath, [script, ...args], { cwd: dir, encoding: 'utf8' });
    assert.equal(result.status, 2);
    assert.match(result.stderr, error);
  }
}));
