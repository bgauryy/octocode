// Hermetic fresh classification, retry, accounting and selection-plan checks on built hosts.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { checks, startServer, writeResults } from './mcp-client.mjs';
import { collect } from './responses.mjs';
import { answer, setup } from './classification-fixture.mjs';
import { canon } from './lead-predicates.mjs';

const { check, summary } = checks('classification-retry');
let mode = 'success';
let remainingFailures = 0;
const ctx = await setup({ label: 'fresh-retry', classificationResponder: async request => {
  if (['slow', 'unauthorized-slow'].includes(mode)) await new Promise(resolve => setTimeout(resolve, 150));
  if (['unauthorized', 'unauthorized-slow'].includes(mode)) return { status: 401, body: { error: 'fixture authentication failure' } };
  if (mode === 'malformed') return { body: '{invalid' };
  if (mode === 'throttle' && remainingFailures-- > 0) return { status: 429, body: { error: 'fixture throttle' } };
  const body = answer(request);
  for (const value of Object.values(body.answers)) {
    if (value.type === 'noul') value.noul = mode === 'changed' ? 0.7 : request.state?.x === null ? 0.9 : 0.1;
  }
  return { body };
} });
// Session stats (persistent storage only) are the accounting surface for
// provider requests: a row carries the provider's usage, not request counts.
const env = { ...ctx.env, OCTOCODE_STORAGE_MODE: 'persistent', OCTOCODE_ENABLE_STATS: 'true' };
const statsFile = path.join(env.OCTOCODE_HOME, 'stats.json');
const usageTotals = () => {
  try { return JSON.parse(fs.readFileSync(statsFile, 'utf8')).stats?.clasify ?? {}; } catch { return {}; }
};
const usageSince = before => {
  const now = usageTotals();
  return Object.fromEntries(['calls', 'known_usage_calls', 'unknown_usage_calls', 'input_tokens', 'output_tokens']
    .map(key => [key, (now[key] ?? 0) - (before[key] ?? 0)]));
};
let client;
const traces = [];
const query = (state, instructions = 'Does the evidence contain an explicit null field?') => ({
  operation: 'evaluate', content: state,
  questions: { answer: { type: 'noul', instructions } },
});
const verdicts = result => collect(result.json ?? result.sc, value => value.type === 'noul' && typeof value.noul === 'number').map(value => value.noul);
const row = result => (result.json ?? result.sc)?.results?.[0]?.data;
const errors = result => collect(result.json ?? result.sc, value => typeof value.errorCode === 'string').map(value => value.errorCode);
const account = key => ({ OCTOCODE_CLASSIFICATION_API: key, OCTOCODE_STORAGE_MODE: 'persistent', OCTOCODE_ENABLE_STATS: 'true' });
const call = async (input, label) => {
  const before = ctx.calls();
  const result = await client.call('clasify', input, {}, label);
  const calls = ctx.calls() - before;
  traces.push({ label, result, providerCalls: calls });
  return { result, calls };
};

try {
  client = await startServer({ env, cwd: ctx.fixture });
  assert.ok(client.tools.some(tool => tool.name === 'clasify'), 'clasify must be available');

  const first = await call(query({ stable: 'evidence', x: null }), 'explicit null');
  check('cold judgment sends one request and returns its answer', first.calls === 1 && verdicts(first.result).includes(0.9));
  mode = 'changed';
  const same = await call(query({ stable: 'evidence', x: null }), 'same exact evidence');
  mode = 'success';
  check('repeated judgment makes a fresh provider request', same.calls === 1 && row(same.result)?.replayed === undefined && row(first.result)?.replayed === undefined && verdicts(same.result).includes(0.7));
  const missing = await call(query({ stable: 'evidence' }), 'missing field');
  check('removing an explicit null invalidates the judgment', missing.calls === 1 && verdicts(missing.result).includes(0.1), JSON.stringify({ calls: missing.calls, errors: errors(missing.result) }));
  const ordered = await call(query({ x: null, nested: { b: 2, a: 1 } }), 'object order original');
  const reordered = await call(query({ nested: { a: 1, b: 2 }, x: null }), 'object order reversed');
  // The provider receives the row's own state in its own key order, so a
  // reordered objects remain independent provider input.
  check('a reordered state object is judged fresh, not replayed', ordered.calls === 1 && reordered.calls === 1 && row(reordered.result)?.replayed === undefined,
    JSON.stringify({ ordered: ordered.calls, reordered: reordered.calls, replayed: row(reordered.result)?.replayed ?? false }));
  const changedQuestion = await call(query({ stable: 'evidence', x: null }, 'Is the x field present with a null value?'), 'different question');
  check('changing the question requires a fresh judgment', changedQuestion.calls === 1 && verdicts(changedQuestion.result).includes(0.9));

  mode = 'slow';
  const beforeWave = ctx.calls();
  const waveUsage = usageTotals();
  const wave = await Promise.all(Array.from({ length: 4 }, () => client.call('clasify', query({ wave: 'success' }))));
  check('concurrent identical callers each make a provider request', ctx.calls() - beforeWave === wave.length && wave.every(result => verdicts(result).includes(0.1)));
  const fresh = wave.filter(result => row(result)?.replayed !== true).length;
  const waveCharged = usageSince(waveUsage);
  check('concurrent usage counts every successful request', fresh === wave.length && waveCharged.calls === wave.length && waveCharged.input_tokens === wave.length * 10, JSON.stringify({ fresh, waveCharged }));

  mode = 'unauthorized-slow';
  const beforeFailedWave = ctx.calls();
  const failedUsage = usageTotals();
  const failedWave = await Promise.all(Array.from({ length: 4 }, () => client.call('clasify', query({ wave: 'failed' }))));
  check('concurrent identical callers each receive a provider failure', ctx.calls() - beforeFailedWave === failedWave.length && failedWave.every(result => errors(result).includes('classificationProviderError')));
  const failedCharged = usageSince(failedUsage);
  check('failed concurrent usage counts every attempted request', failedCharged.calls === failedWave.length, JSON.stringify(failedCharged));

  mode = 'throttle';
  remainingFailures = 1;
  const retryStart = ctx.calls();
  const retryUsage = usageTotals();
  const retried = await call(query({ retry: 'throttle' }), '429 then success');
  const packets = ctx.mock.classificationRequests.slice(retryStart);
  check('transient failure retries once with identical provider input', retried.calls === 2 && packets.length === 2 && canon(packets[0]) === canon(packets[1]) && verdicts(retried.result).includes(0.1));
  // The row shows the provider's reported usage; the throttled attempt
  // reported none, so it is counted as an attempt with unknown usage.
  const retryCharged = usageSince(retryUsage);
  check('retry usage counts both attempts without inventing missing tokens',
    canon(row(retried.result)?.usage) === canon({ input_tokens: 10, output_tokens: 2 })
      && retryCharged.calls === 2 && retryCharged.known_usage_calls === 1 && retryCharged.unknown_usage_calls === 1 && retryCharged.input_tokens === 10,
    JSON.stringify({ usage: row(retried.result)?.usage, retryCharged }));
  // Retries are a runtime constant (3); a varied budget is covered by
  // transport::tests::the_retry_budget_bounds_attempts_exactly.
  remainingFailures = 10;
  const beforeExhausted = ctx.calls();
  const exhausted = await ctx.cli('clasify', query({ retry: 'exhausted' }));
  check('exhausted retry budget stops after the initial attempt plus three retries', ctx.calls() - beforeExhausted === 4 && errors(exhausted).includes('classificationRateLimited'), JSON.stringify({ calls: ctx.calls() - beforeExhausted, errors: errors(exhausted) }));

  mode = 'unauthorized';
  const rejected = await call(query({ retry: 'auth' }), '401 terminal');
  check('authentication failure is terminal despite retry allowance', rejected.calls === 1 && errors(rejected.result).includes('classificationProviderError') && rejected.result.text.includes('401'), JSON.stringify(errors(rejected.result)));
  mode = 'success';
  const repaired = await call(query({ retry: 'auth' }), 'provider recovered');
  check('failed wave is not a persistent negative cache', repaired.calls === 1 && verdicts(repaired.result).includes(0.1));

  mode = 'malformed';
  const malformed = await call(query({ retry: 'invalid-response' }), 'invalid response');
  check('invalid provider response is not retried', malformed.calls === 1 && errors(malformed.result).includes('invalidClassificationResponse'));
  mode = 'success';
  const corrected = await call(query({ retry: 'invalid-response' }), 'valid response later');
  check('invalid provider response is not cached', corrected.calls === 1 && verdicts(corrected.result).includes(0.1));

  const persistentBefore = ctx.calls();
  const persistentUsage = usageTotals();
  const persistentFirst = await ctx.cli('clasify', query({ cli: 'persistent' }), { OCTOCODE_STORAGE_MODE: 'persistent', OCTOCODE_ENABLE_STATS: 'true' });
  const persistentColdCalls = ctx.calls() - persistentBefore;
  const persistentSecond = await ctx.cli('clasify', query({ cli: 'persistent' }), { OCTOCODE_STORAGE_MODE: 'persistent', OCTOCODE_ENABLE_STATS: 'true' });
  const persistentWarmCalls = ctx.calls() - persistentBefore - persistentColdCalls;
  check('persistent storage still evaluates every fresh CLI call', persistentColdCalls === 1 && persistentWarmCalls === 1
    && verdicts(persistentFirst).includes(0.1) && verdicts(persistentSecond).includes(0.1)
    && row(persistentFirst)?.replayed !== true && row(persistentSecond)?.replayed !== true,
  JSON.stringify({ persistentColdCalls, persistentWarmCalls }));
  const persistentCharged = usageSince(persistentUsage);
  check('persistent CLI calls each charge their provider usage',
    canon(row(persistentSecond)?.usage) === canon(row(persistentFirst)?.usage)
      && canon(persistentCharged) === canon({ calls: 2, known_usage_calls: 2, unknown_usage_calls: 0, input_tokens: 20, output_tokens: 4 }),
  JSON.stringify(persistentCharged));

  const memoryCalls = [];
  const memoryResults = [];
  for (let i = 0; i < 2; i++) {
    const before = ctx.calls();
    memoryResults.push(await ctx.cli('clasify', query({ cli: 'memory' }), { OCTOCODE_STORAGE_MODE: 'memory' }));
    memoryCalls.push(ctx.calls() - before);
  }
  check('memory judgments do not survive fresh CLI processes', memoryCalls.every(calls => calls === 1)
    && memoryResults.every(result => verdicts(result).includes(0.1) && row(result)?.replayed !== true), JSON.stringify(memoryCalls));

  // Every account and every process submits its own provider input.
  const beforeAccounts = ctx.calls();
  for (const key of ['fixture-account-a', 'fixture-account-b']) {
    const result = await ctx.cli('clasify', query({ account: 'same-state' }), account(key));
    assert.ok(verdicts(result).includes(0.1), JSON.stringify(result));
  }
  check('different accounts judge independently', ctx.calls() - beforeAccounts === 2);
  const beforeWarmAccount = ctx.calls();
  const warmAccount = await ctx.cli('clasify', query({ account: 'same-state' }), account('fixture-account-a'));
  check('returning to the original account still evaluates fresh', ctx.calls() === beforeWarmAccount + 1 && verdicts(warmAccount).includes(0.1));

} finally {
  client?.close();
  await ctx.close();
}
const result = summary();
writeResults('classification-retry', { ...result, traces });
process.exitCode = result.failed.length ? 1 : 0;
