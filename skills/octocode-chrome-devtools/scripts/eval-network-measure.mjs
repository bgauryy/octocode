#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail: String(detail).slice(0, 1500) });
const port = '9312';
const open = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--headless', '--port', port, '--url', 'about:blank'], { encoding: 'utf8', timeout: 60000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);
const run = spawnSync(
  process.execPath,
  ['skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs', 'skills/octocode-chrome-devtools/scripts/cdp-checks/network-measure-check.mjs', '--port', port, '--new-tab', 'about:blank', '--timeout', '30000', '--script-timeout', '45000'],
  { encoding: 'utf8', timeout: 70000, maxBuffer: 5 * 1024 * 1024 }
);
assert('network measure script runs', run.status === 0, run.stderr.slice(0, 1200) || run.stdout.slice(0, 1200));
assert('prints NET health metric', /\[METRIC\] NET health=\d+/.test(run.stdout), run.stdout.slice(0, 1500));
assert('sees API or HTTP failure finding', /\[FINDING\] NET_|\[NETWORK_ERROR\]/.test(run.stdout), run.stdout.slice(0, 1500));
const artLine = run.stdout.split(/\n/).find(l => l.includes('[ARTIFACT] NETWORK_MEASURE')) || '';
const artifact = artLine.replace(/^.*\[ARTIFACT\] NETWORK_MEASURE\s+/, '').trim();
assert('artifact path emitted', artifact && existsSync(artifact), artifact);
if (artifact && existsSync(artifact)) {
  const data = JSON.parse(readFileSync(artifact, 'utf8'));
  assert('counts.requests >= 1', (data.counts?.requests || 0) >= 1, JSON.stringify(data.counts));
  assert('byKind present', data.byKind && typeof data.byKind === 'object', JSON.stringify(data.byKind));
  assert('captures 404 or api sample', (data.failures?.length || 0) >= 1 || data.sample?.some(s => /example\.test/.test(s.url)), JSON.stringify(data.failures?.slice(0, 2) || data.sample?.slice(0, 2)));
}
spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter(c => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
