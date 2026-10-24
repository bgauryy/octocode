import test from 'node:test';
import assert from 'node:assert/strict';
import { summarize } from './jev_proposal_poc_summary.mjs';

test('summary preserves failed cases and unknown usage and uses paired ratios', () => {
  const results = [
    { case: 'a', arm: 'native-2k', coverage: true, page: 2, responseChars: 100, providerCalls: 1, inputTokens: 10 },
    { case: 'b', arm: 'native-2k', coverage: true, page: 2, responseChars: 1000, providerCalls: 1, inputTokens: 10 },
    { case: 'a', arm: 'candidate', coverage: true, page: 1, responseChars: 50, providerCalls: 1, inputTokens: null },
    { case: 'b', arm: 'candidate', error: 'timeout' },
  ];
  const candidate = summarize({ mode: 'live', results }).arms.find(x => x.arm === 'candidate');
  assert.equal(candidate.attempted, 2);
  assert.equal(candidate.failed, 1);
  assert.equal(candidate.inputTokens, null);
  assert.equal(candidate.medianPairedResponseRatio, 0.5);
});
