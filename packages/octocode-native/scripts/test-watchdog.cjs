#!/usr/bin/env node
'use strict';

/**
 * The target runner of `cargo dev-test`: libtest has no per-test timeout, so a
 * stuck test would hold the run for good and read as slowness. libtest says when
 * a test passes 60 seconds; a test still unfinished this long after that fails
 * the run, by name. `--test-threads=1` prints no such line, so it stays unguarded.
 *
 * `cargo nextest` (`test:rust`, CI) kills a hung test itself (.config/nextest.toml).
 * OCTOCODE_TEST_STUCK_SECONDS overrides the wait.
 */

const { spawn } = require('node:child_process');

/** Well past the slowest test seen with every test binary sharing one machine. */
const STUCK_SECONDS = 600;
/** After the test binary exited: time for its last output to arrive. */
const PIPE_DRAIN_MS = 2000;
const RUNNING = /^test (.+) has been running for over \d+ seconds$/;

const stuckAfterMs = (Number(process.env.OCTOCODE_TEST_STUCK_SECONDS) || STUCK_SECONDS) * 1000;
const [binary, ...args] = process.argv.slice(2);
const child = spawn(binary, args, { stdio: ['inherit', 'pipe', 'inherit'] });
const running = new Map();
let partial = '';

function read(line) {
  const started = RUNNING.exec(line);
  if (started) running.set(started[1], Date.now());
  else for (const name of running.keys()) if (line.startsWith(`test ${name} `)) running.delete(name);
}

child.stdout.on('data', chunk => {
  process.stdout.write(chunk);
  const lines = (partial + chunk).split('\n');
  partial = lines.pop();
  lines.forEach(read);
});

const watch = setInterval(() => {
  for (const [name, since] of running) {
    if (Date.now() - since < stuckAfterMs) continue;
    console.error(`test-watchdog: ${name} is stuck, ${stuckAfterMs / 1000}s past libtest's slow-test line; killing ${binary}`);
    child.kill('SIGKILL');
    process.exitCode = 101;
    process.exit();
  }
}, Math.min(stuckAfterMs, 5000));

child.on('error', error => {
  console.error(`test-watchdog: ${binary}: ${error.message}`);
  process.exit(101);
});
const finish = (code, signal) => {
  clearInterval(watch);
  process.exit(signal ? 101 : code);
};
child.on('close', finish);
// `close` waits for the pipe, which a helper a test left behind may hold for good.
child.on('exit', (code, signal) => {
  clearInterval(watch);
  setTimeout(finish, PIPE_DRAIN_MS, code, signal);
});
