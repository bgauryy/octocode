import assert from 'node:assert/strict';
import test from 'node:test';
import { toolResult } from './run-tool.mjs';

function run(performanceCheck) {
  return {
    suite: { performance: performanceCheck }, name: 'lspSearch', spec: { budgetMs: 25_000 },
    timings: [{ phase: 'happy', surface: 'cli', ms: 100_000, responseBytes: 100 }],
    checks: [], reports: { cli: [], mcp: [] }, skips: [], advancedCalls: [],
    ctx: { mock: { unhandledFrom: [] } }, record(check, ok, detail) { this.checks.push({ check, ok, detail }); },
  };
}

test('default correctness checks never fail on host wall-clock latency', () => {
  const result = toolResult(run(false), 0);
  assert.equal(result.status, 'PASS');
  assert.equal(result.checks.some(c => c.check === 'latency'), false);
  assert.equal(result.budgetMs, undefined);
});

test('explicit performance mode retains the latency oracle with supplied measurements', () => {
  const result = toolResult(run(true), 0);
  assert.equal(result.status, 'FAIL');
  assert.equal(result.checks.find(c => c.check === 'latency').ok, false);
  assert.equal(result.budgetMs, 25_000);
});
