#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });
const port = '9292';
const open = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--headless', '--port', port, '--url', 'about:blank'], { encoding: 'utf8', timeout: 60000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);
const html = encodeURIComponent('<!doctype html><button id="go">Search</button><input name="q" value="cookies"><a rel="next" href="/page/2">Next</a>');
const run = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs', 'skills/octocode-chrome-devtools/examples/graph-actionability-check.mjs', '--port', port, '--new-tab', `data:text/html,${html}`, '--timeout', '30000', '--script-timeout', '45000'], { encoding: 'utf8', timeout: 70000, maxBuffer: 5 * 1024 * 1024 });
assert('actionability script runs', run.status === 0, run.stderr.slice(0, 1000));
assert('prints ACTIONABILITY rows', run.stdout.includes('[ACTIONABILITY]'), run.stdout.slice(0, 1000));
const artifactLine = run.stdout.split(/\n/).find(l => l.includes('[ARTIFACT] ACTIONABILITY')) || '';
const artifact = artifactLine.replace(/^.*\[ARTIFACT\] ACTIONABILITY\s+/, '').trim();
assert('artifact path emitted', artifact && existsSync(artifact), artifact);
if (artifact && existsSync(artifact)) {
  const data = JSON.parse(readFileSync(artifact, 'utf8'));
  assert('button or input rows captured', data.rows?.some(r => ['button', 'input', 'a'].includes(r.tag)), JSON.stringify(data.rows?.slice(0, 3)));
  assert('at least one operable candidate', data.rows?.some(r => r.canOperate === true), JSON.stringify(data.rows?.slice(0, 3)));
}
spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter(c => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
