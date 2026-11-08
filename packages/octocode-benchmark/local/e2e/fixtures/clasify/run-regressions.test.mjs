// Runner logic for regressions.json with a mocked clasify: no process, provider or network.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { buildQuery, FIXTURE_FILE, meetsBound, observedLabel, resolvePath, runRegressions, scoreCall } from './run-regressions.mjs';

const fixture = JSON.parse(fs.readFileSync(FIXTURE_FILE, 'utf8'));
const answered = answer => ({ results: [{ data: { answers: { answer }, usage: { calls: 1, inputTokens: 10 } } }] });
const refusal = { error: 'Tool clasify is off', errorCode: 'missingConfiguration' };

test('every fixture source resolves to a file beside regressions.json', () => {
  for (const testCase of fixture.cases) {
    const relative = resolvePath(testCase.source.path);
    assert.ok(!relative.includes('{{'), relative);
    assert.ok(fs.existsSync(new URL(`../../../../../../${relative}`, import.meta.url)), relative);
  }
});

test('buildQuery sends one evaluate row with the call as its only question', () => {
  const testCase = fixture.cases.find(entry => entry.calls.some(call => call.type === 'score'));
  const call = testCase.calls.find(entry => entry.type === 'score');
  const [row] = buildQuery(testCase, call).queries;
  assert.equal(row.operation, 'evaluate');
  assert.equal(row.source.tool, testCase.source.tool);
  assert.deepEqual(row.questions, { answer: { type: 'score', instructions: call.instructions, criteria: call.criteria } });
  const noul = testCase.calls.find(entry => entry.type === 'noul');
  assert.equal('criteria' in buildQuery(testCase, noul).queries[0].questions.answer, false);
});

test('meetsBound reads each comparison and rejects a missing value', () => {
  assert.equal(meetsBound(0.6, '>0.5'), true);
  assert.equal(meetsBound(0.5, '>0.5'), false);
  assert.equal(meetsBound(0.5, '>=0.5'), true);
  assert.equal(meetsBound(0.2, '<0.3'), true);
  assert.equal(meetsBound(undefined, '>0.5'), false);
});

test('observedLabel maps each answer type to a label', () => {
  assert.equal(observedLabel({ type: 'noul' }, { noul: 0.9 }), 'yes');
  assert.equal(observedLabel({ type: 'noul' }, { noul: 0.1 }), 'no');
  assert.equal(observedLabel({ type: 'choice' }, { choice: 'early_leaks' }), 'early_leaks');
  const score = { type: 'score', criteria: ['low', 'mid', 'high'] };
  assert.equal(observedLabel(score, { score: 1.7, probabilities: [0.1, 0.1, 0.8] }), 'high');
  assert.equal(observedLabel(score, { score: 1.7, probabilities: { high: 0.8, low: 0.2 } }), 'high');
  assert.equal(observedLabel(score, { score: 0.9 }), 'mid');
});

test('scoreCall needs both the label and the P(yes) bound for noul', () => {
  const call = { type: 'noul', expect: { label: 'yes', noul: '>0.5' } };
  assert.equal(scoreCall(call, { noul: 0.8 }).pass, true);
  assert.equal(scoreCall(call, { noul: 0.03 }).pass, false);
});

test('runRegressions scores every call against expect and sums provider calls', async () => {
  const sent = [];
  const truth = { noul: { noul: 0.9 }, score: { score: 2, probabilities: [0, 0.1, 0.9] }, choice: { choice: 'early_leaks' } };
  const run = await runRegressions({ fixture, invoke: async envelope => { sent.push(envelope); return answered(truth[envelope.queries[0].questions.answer.type]); } });
  const total = fixture.cases.reduce((sum, testCase) => sum + testCase.calls.length, 0);
  assert.equal(sent.length, total);
  assert.equal(run.providerCalls, total);
  assert.equal(run.skipped, false);
  assert.ok(run.results.every(result => result.status === 'pass'), JSON.stringify(run.results));
});

test('a wrong answer fails its case and an answerless error is reported', async () => {
  const run = await runRegressions({ fixture, invoke: async envelope => (envelope.queries[0].questions.answer.type === 'choice' ? { errorCode: 'classificationProviderError' } : answered({ noul: 0.02, score: 0, probabilities: [1, 0, 0] })) });
  const byId = Object.fromEntries(run.results.map(result => [result.id, result]));
  assert.equal(byId['F1-numeric-threshold'].status, 'fail');
  assert.equal(byId['C5-lock-held-on-early-return'].status, 'error');
  assert.equal(byId['C5-lock-held-on-early-return'].calls[0].errorCode, 'classificationProviderError');
});

test('without a key the first refusal skips every remaining call without invoking again', async () => {
  let invocations = 0;
  const run = await runRegressions({ fixture, invoke: async () => { invocations += 1; return refusal; } });
  assert.equal(invocations, 1);
  assert.equal(run.skipped, true);
  assert.equal(run.providerCalls, 0);
  assert.ok(run.results.every(result => result.status === 'skipped' && result.calls.every(call => call.status === 'skipped')));
});

test('--case selects only the named cases', async () => {
  const run = await runRegressions({ fixture, only: ['C5-lock-held-on-early-return'], invoke: async () => answered({ choice: 'early_leaks' }) });
  assert.deepEqual(run.results.map(result => result.id), ['C5-lock-held-on-early-return']);
});
