#!/usr/bin/env node
// Runs every hermetic check in its own process, several at a time. Checks share no fixed
// ports or state: each works in its own mkdtemp folder and binds port 0, so order is free.
// Output is buffered per check and printed whole; the first failure sets the exit code.
import { spawn } from 'node:child_process';
import { availableParallelism } from 'node:os';
import { join } from 'node:path';

const scripts = import.meta.dirname;
if (process.argv.includes('--help') || process.argv.includes('-h')) {
  console.log(
    'Usage: hermetic-suite.mjs [--serial]\n\nRuns the browser-free one-folder portability and optional scraping bridge integration checks, in parallel unless --serial.'
  );
  process.exit(0);
}
const checks = [
  'architecture.test.mjs',
  'unit/copy-package.test.mjs',
  'unit/connection.test.mjs',
  'unit/invocation.test.mjs',
  'unit/cli.test.mjs',
  'unit/cleanup.test.mjs',
  'unit/real-profile.test.mjs',
  'unit/cookie-jar.test.mjs',
  'unit/helper-inputs.test.mjs',
  'unit/engine-io.test.mjs',
  'unit/session-store.test.mjs',
  'unit/session-command.test.mjs',
  'unit/plan.test.mjs',
  'unit/plan-targets.test.mjs',
  'unit/detached-ref.test.mjs',
  'unit/snapshot-resource.test.mjs',
  'unit/post-action-wait.test.mjs',
  'unit/evidence.test.mjs',
  'unit/agent.test.mjs',
  'unit/agent-mcp.test.mjs',
  'unit/capture.test.mjs',
  'unit/capture-flow.test.mjs',
  'unit/package.test.mjs',
  'unit/sandbox-layout.test.mjs',
  'unit/stealth-identity.test.mjs',
  'sandbox-env-self-test.mjs',
  'portability-self-test.mjs',
  'artifact-self-test.mjs',
  'robustness-self-test.mjs',
  'source-pagination-self-test.mjs',
  'executor-self-test.mjs',
];
const width = process.argv.includes('--serial')
  ? 1
  : Math.max(1, Math.min(6, Math.floor(availableParallelism() / 2)));

const runCheck = check =>
  new Promise(resolve => {
    const started = Date.now();
    const output = [];
    const child = spawn(process.execPath, [join(scripts, check)], {
      env: { ...process.env },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    child.stdout.on('data', chunk => output.push(chunk));
    child.stderr.on('data', chunk => output.push(chunk));
    child.on('close', (code, signal) => {
      process.stdout.write(Buffer.concat(output));
      const seconds = ((Date.now() - started) / 1000).toFixed(1);
      console.log(`# ${check}: ${code === 0 ? 'ok' : 'FAILED'} (${seconds}s)`);
      resolve({ check, code: code ?? (signal ? 1 : 0) });
    });
  });

const queue = [...checks];
const failed = [];
await Promise.all(
  Array.from({ length: width }, async () => {
    for (let check = queue.shift(); check; check = queue.shift()) {
      const { code } = await runCheck(check);
      if (code !== 0) failed.push(check);
    }
  })
);
if (failed.length) {
  console.error(`hermetic checks failed: ${failed.join(', ')}`);
  process.exit(1);
}
console.log(
  JSON.stringify({
    ok: true,
    suite: 'chrome-devtools-hermetic',
    checks: checks.length,
    parallel: width,
  })
);
