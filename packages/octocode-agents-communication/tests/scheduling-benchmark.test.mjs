import assert from 'node:assert/strict';
import test from 'node:test';
import { benchmarkModels, completeUsage, delta, gradeWorker, schedule, usage } from '../scripts/scheduling-benchmark.mjs';

test('live benchmark requires an explicit Pi model before allocating a run', () => {
  assert.throws(() => benchmarkModels({}), /Set COMMUNICATION_PI_MODEL/);
  assert.throws(() => benchmarkModels({ COMMUNICATION_PI_MODEL: '  ' }), /Set COMMUNICATION_PI_MODEL/);
  assert.deepEqual(benchmarkModels({ COMMUNICATION_PI_MODEL: 'provider/model' }),
    { codex: 'gpt-6-luna', claude: 'haiku', pi: 'provider/model' });
});

test('Codex cumulative samples are not summed; unknown cache writes remain unknown', () => {
  const events = [10, 20].map(inputTokens => ({ type: 'usage', scope: 'thread',
    usage: { total: { inputTokens, cachedInputTokens: 8, outputTokens: 3 } } }));
  const value = usage('codex', events);
  assert.deepEqual(value, { input: 20, output: 3, cacheRead: 8, cacheWrite: null });
  assert.deepEqual(delta(value, { input: 10, output: 1, cacheRead: 4, cacheWrite: null }),
    { input: 10, output: 2, cacheRead: 4, cacheWrite: null });
});

test('Claude and Pi count each authoritative usage scope once, including cache input', () => {
  assert.deepEqual(usage('claude', [
    { type: 'usage', scope: 'message', usage: { input_tokens: 999 } },
    ...[1, 2].map(() => ({ type: 'usage', scope: 'result', usage:
      { input_tokens: 2, cache_read_input_tokens: 3, cache_creation_input_tokens: 4, output_tokens: 5 } })),
  ]), { input: 18, output: 10, cacheRead: 6, cacheWrite: 8 });
  assert.deepEqual(usage('pi', [
    { type: 'usage', scope: 'message', usage: { input: 2, cacheRead: 3, cacheWrite: 4, output: 5 } },
  ]), { input: 9, output: 5, cacheRead: 3, cacheWrite: 4 });
});

test('balanced order matches two pairs without retry or dropped arm', () => {
  assert.deepEqual(schedule(), [{ pair: 1, arm: 'baseline' }, { pair: 1, arm: 'candidate' },
    { pair: 2, arm: 'candidate' }, { pair: 2, arm: 'baseline' }]);
});

test('absent snapshots and missing fields are unknown, observed zeros remain valid', () => {
  for (const vendor of ['codex', 'claude', 'pi']) {
    const missing = usage(vendor, []);
    assert.equal(missing.input, null);
    assert.equal(completeUsage(vendor, missing), false);
    assert.equal(delta(missing, missing).input, null);
  }
  assert.equal(completeUsage('pi', usage('pi', [{ type: 'usage', scope: 'message',
    usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 } }])), true);
  assert.equal(usage('claude', [{ type: 'usage', scope: 'result', usage:
    { input_tokens: 0, output_tokens: 0, cache_read_input_tokens: 0 } }]).input, null);
});

test('correctness rejects duplicates, missing acknowledgements and passive wakeups', () => {
  const input = { arm: 'candidate', startupTurns: 1, turns: 2, beforeProcessTurns: 1,
    messages: [{ body: 'DONE' }], acknowledgements: [{ acknowledgedAt: 1 }, { acknowledgedAt: 2 }], batches: [[4, 5]] };
  assert.deepEqual(gradeWorker(input), { correct: true, noPassiveWake: true, handlingTurns: 1 });
  assert.equal(gradeWorker({ ...input, batches: [[4, 5], [4]] }).correct, false);
  assert.equal(gradeWorker({ ...input, messages: [{ body: 'DONE' }, { body: 'DONE' }] }).correct, false);
  assert.equal(gradeWorker({ ...input, acknowledgements: [{ acknowledgedAt: null }, { acknowledgedAt: 2 }] }).correct, false);
  assert.equal(gradeWorker({ ...input, beforeProcessTurns: 2 }).noPassiveWake, false);
});
