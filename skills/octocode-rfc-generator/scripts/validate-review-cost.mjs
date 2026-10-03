#!/usr/bin/env node

import assert from 'node:assert/strict';
import { readFileSync, realpathSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const text = value => typeof value === 'string' && value.trim().length > 0;
const count = value => Number.isInteger(value) && value >= 0;
const measure = value => value === null || (typeof value === 'number' && Number.isFinite(value) && value >= 0);
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const only = (value, keys) => object(value) && Object.keys(value).every(key => keys.includes(key));
const CONTRIBUTIONS = new Set([
  'changed-action', 'prioritized-existing-concern', 'confirmation-only',
  'no-demonstrated-help', 'harmful',
]);

function metricPaths(receipt) {
  const paths = [
    ['phases.preparationMs', receipt?.phases?.preparationMs],
    ['phases.debateMs', receipt?.phases?.debateMs],
    ['phases.judgeMs', receipt?.phases?.judgeMs],
    ['phases.verificationMs', receipt?.phases?.verificationMs],
    ['host.inputTokens', receipt?.host?.inputTokens],
    ['host.outputTokens', receipt?.host?.outputTokens],
    ['judge.elapsedMs', receipt?.judge?.elapsedMs],
    ['judge.inputTokens', receipt?.judge?.inputTokens],
    ['judge.outputTokens', receipt?.judge?.outputTokens],
  ];
  for (const worker of receipt?.workers ?? []) {
    paths.push(
      [`workers.${worker?.id}.elapsedMs`, worker?.elapsedMs],
      [`workers.${worker?.id}.inputTokens`, worker?.inputTokens],
      [`workers.${worker?.id}.outputTokens`, worker?.outputTokens],
    );
  }
  return paths;
}

export function validateReviewCost(receipt) {
  const errors = [];
  if (receipt?.version !== 1 || !text(receipt?.reviewId) || receipt?.scope !== 'single-rfc-review') {
    errors.push('Receipt needs version:1, reviewId, and scope:single-rfc-review.');
  }
  const timing = receipt?.timing;
  if (![timing?.startedAtMs, timing?.completedAtMs, timing?.elapsedMs].every(measure) ||
      timing?.startedAtMs === null || timing?.completedAtMs === null || timing?.elapsedMs === null ||
      timing.completedAtMs < timing.startedAtMs || timing.elapsedMs !== timing.completedAtMs - timing.startedAtMs) {
    errors.push('timing requires exact nonnegative startedAtMs/completedAtMs/elapsedMs coverage.');
  }
  const phases = receipt?.phases;
  for (const phase of ['preparationMs', 'debateMs', 'judgeMs', 'verificationMs']) {
    if (!measure(phases?.[phase])) errors.push(`phases.${phase} must be a nonnegative number or null.`);
  }
  if (!count(receipt?.host?.toolCalls) || !count(receipt?.host?.evidenceReads) ||
      !measure(receipt?.host?.inputTokens) || !measure(receipt?.host?.outputTokens)) {
    errors.push('host requires nonnegative toolCalls/evidenceReads and numeric-or-null token fields.');
  }
  if (!Array.isArray(receipt?.workers) || receipt.workers.length !== 2 ||
      new Set(receipt.workers.map(worker => worker?.id)).size !== 2 ||
      !receipt.workers.every(worker => ['A', 'B'].includes(worker?.id))) {
    errors.push('workers must contain exactly A and B once each.');
  } else {
    for (const worker of receipt.workers) {
      if (!['complete', 'partial', 'failed'].includes(worker.status) || !count(worker.rounds) ||
          !count(worker.toolCalls) || !count(worker.evidenceReads) || !measure(worker.elapsedMs) ||
          !measure(worker.inputTokens) || !measure(worker.outputTokens)) {
        errors.push(`worker ${worker.id} has invalid status, counts, elapsed time, or token fields.`);
      }
    }
  }
  const judge = receipt?.judge;
  if (!only(judge, ['calls', 'attempts', 'requestedModel', 'resolvedModels', 'elapsedMs', 'inputTokens', 'outputTokens', 'outcome']) ||
      !count(judge?.calls) || !count(judge?.attempts) || judge?.attempts < judge?.calls ||
      !measure(judge?.elapsedMs) || !measure(judge?.inputTokens) || !measure(judge?.outputTokens) ||
      !['judgment', 'converged-no-call', 'failed', 'unavailable'].includes(judge?.outcome) ||
      !Array.isArray(judge?.resolvedModels) || !judge.resolvedModels.every(text) ||
      new Set(judge.resolvedModels).size !== judge.resolvedModels.length) {
    errors.push('judge requires valid calls/attempts, measurements, and outcome.');
  }
  if (judge?.attempts > 0 && !text(judge?.requestedModel)) errors.push('judge.requestedModel is required when a provider attempt ran.');
  if (judge?.attempts === 0 && judge?.requestedModel !== null) errors.push('judge.requestedModel must be null when no provider attempt ran.');
  if (judge?.calls === 0 && judge?.resolvedModels?.length !== 0) errors.push('judge.resolvedModels must be empty when no call ran.');
  if (judge?.outcome === 'judgment' && judge?.resolvedModels?.length === 0) errors.push('A successful judgment must record at least one provider-resolved model.');
  if (judge?.calls === 0 && [judge?.elapsedMs, judge?.inputTokens, judge?.outputTokens].some(value => value !== null)) {
    errors.push('judge measurements must be null when no provider call ran.');
  }
  if (judge?.outcome === 'converged-no-call' && judge?.calls !== 0) errors.push('converged-no-call cannot include a provider call.');
  if (!Array.isArray(receipt?.failures) || !receipt.failures.every(text)) errors.push('failures must be a string array.');
  if (!Array.isArray(receipt?.unknowns) || !receipt.unknowns.every(text)) errors.push('unknowns must be a string array.');
  const unknowns = new Set(receipt?.unknowns ?? []);
  const metrics = metricPaths(receipt);
  for (const [metric, value] of metrics) {
    if (value === null && !unknowns.has(metric)) errors.push(`${metric} is null but absent from unknowns.`);
    if (value !== null && unknowns.has(metric)) errors.push(`${metric} is measured but listed as unknown.`);
  }
  for (const unknown of unknowns) {
    if (!metrics.some(([metric]) => metric === unknown)) errors.push(`unknown metric path is not recognized: ${unknown}.`);
  }
  if (!CONTRIBUTIONS.has(receipt?.contribution)) errors.push('contribution classification is invalid.');
  return {
    valid: errors.length === 0,
    errors,
    knownMetrics: metrics.filter(([, value]) => value !== null).map(([metric]) => metric),
    unknownMetrics: metrics.filter(([, value]) => value === null).map(([metric]) => metric),
  };
}

function fixture() {
  return {
    version: 1, reviewId: 'r1', scope: 'single-rfc-review',
    timing: { startedAtMs: 100, completedAtMs: 1300, elapsedMs: 1200 },
    phases: { preparationMs: 200, debateMs: 500, judgeMs: 100, verificationMs: 400 },
    host: { toolCalls: 4, evidenceReads: 3, inputTokens: null, outputTokens: null },
    workers: [
      { id: 'A', status: 'complete', rounds: 2, toolCalls: 2, evidenceReads: 2, elapsedMs: 300, inputTokens: null, outputTokens: null },
      { id: 'B', status: 'complete', rounds: 2, toolCalls: 2, evidenceReads: 2, elapsedMs: 320, inputTokens: null, outputTokens: null },
    ],
    judge: { calls: 1, attempts: 1, requestedModel: 'jev-test', resolvedModels: ['jev-test-2026-09'], elapsedMs: 100, inputTokens: 1000, outputTokens: 90, outcome: 'judgment' },
    failures: [],
    unknowns: ['host.inputTokens', 'host.outputTokens', 'workers.A.inputTokens', 'workers.A.outputTokens', 'workers.B.inputTokens', 'workers.B.outputTokens'],
    contribution: 'prioritized-existing-concern',
  };
}

function selfTest() {
  assert.equal(validateReviewCost(fixture()).valid, true);
  const mutations = [
    value => { value.timing.elapsedMs = 1199; },
    value => { value.workers.pop(); },
    value => { value.workers[1].id = 'A'; },
    value => { value.unknowns.pop(); },
    value => { value.judge.calls = 0; },
    value => { value.judge.requestedModel = null; },
    value => { value.judge.resolvedModels = []; },
    value => { value.judge.model = 'legacy-undifferentiated-model'; },
    value => { value.contribution = 'accuracy-improved'; },
  ];
  for (const mutate of mutations) {
    const broken = structuredClone(fixture());
    mutate(broken);
    assert.equal(validateReviewCost(broken).valid, false);
  }
  const converged = fixture();
  converged.judge = { calls: 0, attempts: 0, requestedModel: null, resolvedModels: [], elapsedMs: null, inputTokens: null, outputTokens: null, outcome: 'converged-no-call' };
  converged.unknowns.push('judge.elapsedMs', 'judge.inputTokens', 'judge.outputTokens');
  assert.equal(validateReviewCost(converged).valid, true);
  return { valid: true, selfTest: true, cases: mutations.length + 2 };
}

if (process.argv[1] && process.argv[1] !== '-' && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  const args = process.argv.slice(2);
  try {
    if (args.length === 1 && args[0] === '--self-test') console.log(JSON.stringify(selfTest()));
    else if (args.length === 1 && args[0] === '--help') console.log('Usage: node validate-review-cost.mjs <receipt.json>\n       node validate-review-cost.mjs --self-test');
    else if (args.length === 1) {
      const result = validateReviewCost(JSON.parse(readFileSync(args[0], 'utf8')));
      console.log(JSON.stringify(result, null, 2));
      process.exitCode = result.valid ? 0 : 1;
    } else throw new Error('Use --help for supported arguments.');
  } catch (error) {
    console.error(JSON.stringify({ valid: false, error: error instanceof SyntaxError ? 'Invalid JSON input.' : error.message }));
    process.exitCode = 2;
  }
}
