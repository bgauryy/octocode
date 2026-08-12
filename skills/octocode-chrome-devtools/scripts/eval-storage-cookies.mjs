#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });
const port = '9293';
const open = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--headless', '--port', port, '--url', 'about:blank'], { encoding: 'utf8', timeout: 60000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);
const html = encodeURIComponent('<!doctype html><script>try{localStorage.setItem("theme","dark");sessionStorage.setItem("step","1");}catch(e){}</script><body>storage</body>');
const run = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs', 'skills/octocode-chrome-devtools/scripts/cdp-checks/storage-cookies-audit.mjs', '--port', port, '--new-tab', `data:text/html,${html}`, '--timeout', '30000', '--script-timeout', '45000'], { encoding: 'utf8', timeout: 70000, maxBuffer: 5 * 1024 * 1024 });
assert('storage audit script runs', run.status === 0, run.stderr.slice(0, 1000));
assert('prints storage metrics', run.stdout.includes('[METRIC] STORAGE'), run.stdout.slice(0, 1000));
const artifactLine = run.stdout.split(/\n/).find(l => l.includes('[ARTIFACT] STORAGE_COOKIES')) || '';
const artifact = artifactLine.replace(/^.*\[ARTIFACT\] STORAGE_COOKIES\s+/, '').trim();
assert('artifact path emitted', artifact && existsSync(artifact), artifact);
if (artifact && existsSync(artifact)) {
  const data = JSON.parse(readFileSync(artifact, 'utf8'));
  assert('cookie metadata has no values', (data.cookies?.rows || []).every(c => !('value' in c)), JSON.stringify(data.cookies?.rows?.[0]));
  assert('storage object returned', data.storage && typeof data.storage === 'object', JSON.stringify(data.storage));
}
spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter(c => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
