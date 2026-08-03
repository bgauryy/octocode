#!/usr/bin/env node
import { spawnSync } from 'node:child_process';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });

// Regression test for a real race: --url at launch time can still leave the
// tab on Chrome's internal about:blank (readyState "complete" for THAT blank
// page) when a script attaches moments later — querySelector/scan then
// silently reports real elements as not-found/zero-rows instead of erroring.
// Reproduces the exact shape of the bug: --url at launch, then an immediate
// attach with no --new-tab/extra navigation step in between.
const fixtureHtml = encodeURIComponent(
  '<!doctype html><body><input id="username"><input id="password" type="password"><button type="submit">Login</button></body>'
);

function freshLaunchAndRun(port, script, extraEnv = {}) {
  const open = spawnSync(process.execPath, [
    'skills/octocode-chrome-devtools/scripts/open-browser.mjs',
    '--headless', '--port', port, '--url', `data:text/html,${fixtureHtml}`,
  ], { encoding: 'utf8', timeout: 60000 });
  const run = spawnSync(process.execPath, [
    'skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs', script,
    '--port', port, '--keep-tab', '--timeout', '20000', '--script-timeout', '25000',
  ], { encoding: 'utf8', timeout: 40000, maxBuffer: 5 * 1024 * 1024, env: { ...process.env, ...extraEnv } });
  spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
  return { open, run };
}

const { open: open1, run: run1 } = freshLaunchAndRun('9660', 'skills/octocode-chrome-devtools/examples/dom-operations-check.mjs', { DOM_SELECTOR: '#username', DOM_ACTION: 'inspect' });
assert('dom-operations-check: launches', open1.status === 0, open1.stderr || open1.stdout);
assert('dom-operations-check: runs', run1.status === 0, run1.stderr.slice(0, 1000));
assert('dom-operations-check: finds #username immediately post-launch (not a false not-found)', run1.stdout.includes('found=true'), run1.stdout.slice(0, 500));

const { open: open2, run: run2 } = freshLaunchAndRun('9661', 'skills/octocode-chrome-devtools/examples/graph-actionability-check.mjs');
assert('graph-actionability-check: launches', open2.status === 0, open2.stderr || open2.stdout);
assert('graph-actionability-check: runs', run2.status === 0, run2.stderr.slice(0, 1000));
const rowsMatch = run2.stdout.match(/\[METRIC\] ACTIONABILITY rows=(\d+)/);
const rows = rowsMatch ? Number(rowsMatch[1]) : -1;
assert('graph-actionability-check: finds rows immediately post-launch (not a false zero)', rows >= 2, run2.stdout.slice(0, 500));

const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
