#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });
const port = '9299';
const open = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--headless', '--port', port, '--url', 'about:blank'], { encoding: 'utf8', timeout: 60000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);
const html = encodeURIComponent('<!doctype html><title>Consent test</title><button>Accept all</button><input name="q"><a href="/next">Next</a><p>cookie consent gdpr privacy choices</p>');
const run = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs', 'skills/octocode-chrome-devtools/scripts/cdp-checks/actionability-diagnostics.mjs', '--port', port, '--new-tab', `data:text/html,${html}`, '--timeout', '30000', '--script-timeout', '45000'], { encoding: 'utf8', timeout: 70000, maxBuffer: 5 * 1024 * 1024 });
assert('diagnostics script runs', run.status === 0, run.stderr.slice(0, 1000));
assert('prints diagnosis', run.stdout.includes('[DIAGNOSIS]'), run.stdout.slice(0, 1000));
const artifactLine = run.stdout.split(/\n/).find(l => l.includes('[ARTIFACT] ACTIONABILITY_DIAGNOSTICS')) || '';
const artifact = artifactLine.replace(/^.*\[ARTIFACT\] ACTIONABILITY_DIAGNOSTICS\s+/, '').trim();
assert('artifact path emitted', artifact && existsSync(artifact), artifact);
if (artifact && existsSync(artifact)) {
  const data = JSON.parse(readFileSync(artifact, 'utf8'));
  assert('classification includes consent-region', data.classification?.includes('consent-region'), JSON.stringify(data.classification));
  assert('counts buttons and inputs', data.counts?.buttons >= 1 && data.counts?.inputs >= 1, JSON.stringify(data.counts));
  assert('screenshot artifact exists', data.screenshot && existsSync(data.screenshot), data.screenshot);
}
spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter(c => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
