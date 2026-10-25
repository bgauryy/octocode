import assert from 'node:assert/strict';
import test from 'node:test';
import { formatCheckSummary, summarizeChecks } from './check-summary.mjs';

test('check summaries distinguish passed, failed and not-applicable records without mutating evidence', () => {
  const checks = Object.freeze([
    Object.freeze({ check: 'output', ok: true, detail: 'valid' }),
    Object.freeze({ check: 'parity', ok: null, na: 'CLI-only' }),
    Object.freeze({ check: 'page', ok: false, detail: 'missing row' }),
    Object.freeze({ check: 'latency', ok: true, detail: 'within budget' }),
  ]);
  const summary = summarizeChecks(checks);
  assert.deepEqual(summary, { total: 4, passed: 2, failed: 1, notApplicable: 1 });
  assert.equal(formatCheckSummary(summary), '2 passed, 1 failed, 1 not applicable (4 records)');
  assert.deepEqual(JSON.parse(JSON.stringify({ checks, checkSummary: summary })), { checks, checkSummary: summary });
});

test('aggregate check summaries retain N/A records without counting them as executed checks', () => {
  const tools = [
    { checks: [{ ok: true }, { ok: null }, { ok: null }] },
    { checks: [{ ok: true }, { ok: true }, { ok: null }, { ok: null }, { ok: null }] },
  ];
  const summary = summarizeChecks(tools.flatMap(tool => tool.checks));
  assert.deepEqual(summary, { total: 8, passed: 3, failed: 0, notApplicable: 5 });
  assert.equal(formatCheckSummary(summary), '3 passed, 0 failed, 5 not applicable (8 records)');
  assert.deepEqual(summarizeChecks([]), { total: 0, passed: 0, failed: 0, notApplicable: 0 });
});
