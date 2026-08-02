#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });
const port = '9294';
const open = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--headless', '--port', port, '--url', 'about:blank'], { encoding: 'utf8', timeout: 60000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);
const run = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs', 'skills/octocode-chrome-devtools/examples/network-body-har-fetch-check.mjs', '--port', port, '--new-tab', 'about:blank', '--timeout', '30000', '--script-timeout', '45000'], { encoding: 'utf8', timeout: 70000, maxBuffer: 5 * 1024 * 1024 });
assert('network body/HAR script runs', run.status === 0, run.stderr.slice(0, 1200));
assert('Fetch interception fulfilled body', run.stdout.includes('[NETWORK_BODY] 200 https://example.test/api/data'), run.stdout.slice(0, 1200));
const harLine = run.stdout.split(/\n/).find(l => l.includes('[ARTIFACT] HAR')) || '';
const bodiesLine = run.stdout.split(/\n/).find(l => l.includes('[ARTIFACT] NETWORK_BODIES')) || '';
const harPath = harLine.replace(/^.*\[ARTIFACT\] HAR\s+/, '').trim();
const bodiesPath = bodiesLine.replace(/^.*\[ARTIFACT\] NETWORK_BODIES\s+/, '').trim();
assert('HAR artifact exists', harPath && existsSync(harPath), harPath);
assert('body artifact exists', bodiesPath && existsSync(bodiesPath), bodiesPath);
if (harPath && existsSync(harPath)) {
  const har = JSON.parse(readFileSync(harPath, 'utf8'));
  assert('HAR has API entry', har.log?.entries?.some(e => e.request?.url === 'https://example.test/api/data' && e.response?.status === 200), JSON.stringify(har.log?.entries?.slice(0, 2)));
}
if (bodiesPath && existsSync(bodiesPath)) {
  const bodies = JSON.parse(readFileSync(bodiesPath, 'utf8'));
  assert('captured response body has expected JSON', bodies.some(b => /"alpha"/.test(b.body)), JSON.stringify(bodies));
}
spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter(c => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
