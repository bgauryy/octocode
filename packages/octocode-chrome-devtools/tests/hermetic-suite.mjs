#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';

const scripts = import.meta.dirname;
if (process.argv.includes('--help') || process.argv.includes('-h')) {
  console.log(
    'Usage: hermetic-suite.mjs\n\nRuns the browser-free one-folder portability and optional scraping bridge integration checks.'
  );
  process.exit(0);
}
const checks = [
  'architecture.test.mjs',
  'unit/connection.test.mjs',
  'unit/invocation.test.mjs',
  'unit/cli.test.mjs',
  'unit/helper-inputs.test.mjs',
  'unit/plan.test.mjs',
  'unit/detached-ref.test.mjs',
  'unit/evidence.test.mjs',
  'unit/agent.test.mjs',
  'unit/capture.test.mjs',
  'unit/package.test.mjs',
  'sandbox-env-self-test.mjs',
  'portability-self-test.mjs',
  'artifact-self-test.mjs',
  'robustness-self-test.mjs',
  'source-pagination-self-test.mjs',
  'executor-self-test.mjs',
];
for (const check of checks) {
  const result = spawnSync(process.execPath, [join(scripts, check)], {
    stdio: 'inherit',
    env: { ...process.env },
  });
  if (result.status !== 0) process.exit(result.status ?? 1);
}
console.log(
  JSON.stringify({
    ok: true,
    suite: 'chrome-devtools-hermetic',
    checks: checks.length,
  })
);
