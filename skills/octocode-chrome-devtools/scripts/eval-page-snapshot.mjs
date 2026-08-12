#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });
const port = '9551';

const fixtureHtml = encodeURIComponent(
  '<!doctype html><body>' +
  '<button>One</button><button>Two</button><a href="#">Three</a>' +
  '<input aria-label="Four">' +
  '</body>'
);

// Regression test: a snapshot taken immediately after launch used to race
// Chrome's internal about:blank commit and silently return an empty tree
// (readyState already "complete" for the blank page before the real
// navigation lands) instead of the real page's accessibility tree.
const open = spawnSync(process.execPath, [
  'skills/octocode-chrome-devtools/scripts/open-browser.mjs',
  '--headless', '--port', port, '--url', `data:text/html,${fixtureHtml}`,
], { encoding: 'utf8', timeout: 60000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);

let launchInfo = null;
try { launchInfo = JSON.parse(open.stdout.trim().split('\n').pop()); } catch {}
assert('default headless window size is 1280x720, not an undersized default', launchInfo?.windowSize === '1280x720', JSON.stringify(launchInfo));

const run = spawnSync(process.execPath, [
  'skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs',
  'skills/octocode-chrome-devtools/scripts/cdp-checks/page-snapshot.mjs',
  '--port', port, '--keep-tab', '--timeout', '20000', '--script-timeout', '25000',
], { encoding: 'utf8', timeout: 40000, maxBuffer: 5 * 1024 * 1024 });
assert('page-snapshot.mjs runs', run.status === 0, run.stderr.slice(0, 1000));

const metricLine = run.stdout.split('\n').find((l) => l.includes('[METRIC] SNAPSHOT')) || '';
const refsMatch = metricLine.match(/refs=(\d+)/);
const refs = refsMatch ? Number(refsMatch[1]) : -1;
assert('immediate post-launch snapshot is not empty (the reproduced race)', refs >= 4, metricLine);
assert('found the fixture\'s 4 interactive elements', refs === 4, metricLine);

const artifactLine = run.stdout.split('\n').find((l) => l.includes('[ARTIFACT] PAGE_SNAPSHOT')) || '';
const artifact = artifactLine.replace(/^.*\[ARTIFACT\] PAGE_SNAPSHOT\s+/, '').trim();
if (artifact && existsSync(artifact)) {
  const data = JSON.parse(readFileSync(artifact, 'utf8'));
  assert('artifact has one ref per fixture element', Object.keys(data.refs || {}).length === 4, JSON.stringify(data.refs));
}

spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
