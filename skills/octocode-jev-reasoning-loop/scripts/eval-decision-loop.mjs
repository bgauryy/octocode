#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { routeDecision, DEFAULT_POLICY } from './decision-contract.mjs';

if (process.argv.slice(2).some(arg => ['--help', '-h'].includes(arg))) {
  console.log('Usage: node scripts/eval-decision-loop.mjs\nRuns the frozen 15-case deterministic routing/policy suite. Semantic Jev quality requires the benchmark protocol in references/benchmark.md.');
  process.exit(0);
}
if (process.argv.length > 2) {
  console.error(JSON.stringify({ error: 'No options are accepted; use --help.' }));
  process.exit(2);
}
const suite = JSON.parse(readFileSync(new URL('../evals/decision-cases.json', import.meta.url), 'utf8'));
const failures = [];
for (const fixture of suite.cases) {
  const actual = routeDecision(fixture.input, DEFAULT_POLICY);
  if (JSON.stringify(actual) !== JSON.stringify(fixture.expected)) failures.push({ id: fixture.id, expected: fixture.expected, actual });
}
const result = { total: suite.cases.length, passed: suite.cases.length - failures.length, failed: failures.length, passRate: (suite.cases.length - failures.length) / suite.cases.length, failures };
console.log(JSON.stringify(result));
process.exitCode = failures.length ? 1 : 0;
