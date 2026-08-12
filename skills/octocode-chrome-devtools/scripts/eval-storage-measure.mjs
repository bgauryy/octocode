#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail: String(detail).slice(0, 1500) });
const port = '9313';
const open = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--headless', '--port', port, '--url', 'about:blank'], { encoding: 'utf8', timeout: 60000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);
const run = spawnSync(
  process.execPath,
  ['skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs', 'skills/octocode-chrome-devtools/scripts/cdp-checks/storage-measure-check.mjs', '--port', port, '--new-tab', 'about:blank', '--timeout', '30000', '--script-timeout', '45000'],
  { encoding: 'utf8', timeout: 70000, maxBuffer: 5 * 1024 * 1024 }
);
assert('storage measure script runs', run.status === 0, run.stderr.slice(0, 1200) || run.stdout.slice(0, 1200));
assert('prints STORAGE health metric', /\[METRIC\] STORAGE health=\d+/.test(run.stdout), run.stdout.slice(0, 1500));
const artLine = run.stdout.split(/\n/).find(l => l.includes('[ARTIFACT] STORAGE_MEASURE')) || '';
const artifact = artLine.replace(/^.*\[ARTIFACT\] STORAGE_MEASURE\s+/, '').trim();
assert('artifact path emitted', artifact && existsSync(artifact), artifact);
if (artifact && existsSync(artifact)) {
  const data = JSON.parse(readFileSync(artifact, 'utf8'));
  assert('cookie rows never include value', (data.cookies?.rows || []).every(c => !('value' in c)), JSON.stringify(data.cookies?.rows?.[0]));
  assert('localStorage has theme key', (data.storage?.localStorageKeys || []).includes('theme'), JSON.stringify(data.storage));
  assert('flags suspicious tracking_id key', (data.storage?.suspiciousLocalKeys || []).includes('tracking_id') || (data.score?.findings || []).some(f => f.code === 'SUSPICIOUS_LOCALSTORAGE_KEYS'), JSON.stringify(data.score?.findings));
  assert('score.health present', typeof data.score?.health === 'number', JSON.stringify(data.score));
}
spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter(c => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
