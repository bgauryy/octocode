#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail: String(detail).slice(0, 1500) });
const port = '9311';
const open = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--headless', '--port', port, '--url', 'about:blank'], { encoding: 'utf8', timeout: 60000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);
const run = spawnSync(
  process.execPath,
  ['skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs', 'skills/octocode-chrome-devtools/scripts/cdp-checks/performance-measure-check.mjs', '--port', port, '--new-tab', 'about:blank', '--timeout', '30000', '--script-timeout', '45000'],
  { encoding: 'utf8', timeout: 70000, maxBuffer: 5 * 1024 * 1024 }
);
assert('performance measure script runs', run.status === 0, run.stderr.slice(0, 1200) || run.stdout.slice(0, 1200));
assert('prints PERF health metric', /\[METRIC\] PERF health=\d+/.test(run.stdout), run.stdout.slice(0, 1500));
const artLine = run.stdout.split(/\n/).find(l => l.includes('[ARTIFACT] PERFORMANCE')) || '';
const artifact = artLine.replace(/^.*\[ARTIFACT\] PERFORMANCE\s+/, '').trim();
assert('artifact path emitted', artifact && existsSync(artifact), artifact);
if (artifact && existsSync(artifact)) {
  const data = JSON.parse(readFileSync(artifact, 'utf8'));
  assert('score.health is 0–100', typeof data.score?.health === 'number' && data.score.health >= 0 && data.score.health <= 100, JSON.stringify(data.score));
  assert('has longTasks or measures from fixture', (data.longTasks?.length || 0) > 0 || (data.measures?.length || 0) > 0 || data.fcp != null, JSON.stringify({ longTasks: data.longTasks?.length, measures: data.measures?.length, fcp: data.fcp }));
}
spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter(c => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
