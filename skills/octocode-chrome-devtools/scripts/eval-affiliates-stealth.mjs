#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail: String(detail).slice(0, 1500) });
const port = process.env.EVAL_AFFILIATES_PORT || '9296';
const url = process.env.AFFILIATES_PROBE_URL || 'https://affiliates.walmart.com/';

const open = spawnSync(process.execPath, [
  'skills/octocode-chrome-devtools/scripts/open-browser.mjs',
  '--headless', '--port', port, '--url', 'about:blank',
], { encoding: 'utf8', timeout: 90000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);

const run = spawnSync(process.execPath, [
  'skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs',
  'skills/octocode-chrome-devtools/scripts/cdp-checks/affiliates-stealth-probe.mjs',
  '--port', port,
  '--new-tab', url,
  '--keep-tab',
  '--timeout', '60000',
  '--script-timeout', '75000',
], { encoding: 'utf8', timeout: 120000, maxBuffer: 6 * 1024 * 1024 });

assert('affiliates stealth probe exits 0', run.status === 0, run.stderr?.slice(0, 800) || run.stdout?.slice(0, 400));
assert('mandatory stealth metric in stdout', /\[METRIC\] stealth self-test: \d+\/15 passed/.test(run.stdout), run.stdout.slice(0, 500));
assert('stealth patches applied', run.stdout.includes('[INJECT] Stealth patches applied (mandatory gate)'), run.stdout.slice(0, 500));
assert('affiliate h1 in stdout', run.stdout.includes('Become a Walmart Affiliate'), run.stdout.slice(0, 500));
const artLine = run.stdout.split('\n').find((l) => l.includes('[ARTIFACT]') && l.includes('affiliates-stealth-probe.json')) || '';
const artPath = artLine.replace(/^.*\[ARTIFACT\]\s*/, '').trim();
assert('artifact exists', artPath && existsSync(artPath), artPath);
if (artPath && existsSync(artPath)) {
  const data = JSON.parse(readFileSync(artPath, 'utf8'));
  assert('artifact records stealth verify', data.stealthVerify?.failed === 0, JSON.stringify(data.stealthVerify));
  assert('signup login href present', data.page?.signupLinks?.some((l) => /account\/login/.test(l.href)), JSON.stringify(data.page?.signupLinks));
}

spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
