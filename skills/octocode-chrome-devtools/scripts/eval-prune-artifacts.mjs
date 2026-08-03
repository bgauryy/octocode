#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, utimesSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });

const base = mkdtempSync(join(tmpdir(), 'prune-artifacts-eval-'));
const now = Date.now();
const day = 24 * 60 * 60 * 1000;

// One expired run dir (10 days old), three fresh run dirs, one non-timestamp dir to ignore.
const makeRunDir = (name, ageMs) => {
  const dir = join(base, name);
  mkdirSync(dir, { recursive: true });
  const t = (now - ageMs) / 1000;
  utimesSync(dir, t, t);
};
makeRunDir('2026-01-01-00-00-00', 10 * day);
makeRunDir('2026-07-01-00-00-00', 1 * day);
makeRunDir('2026-07-02-00-00-00', 1 * day);
makeRunDir('2026-07-03-00-00-00', 1 * day);
makeRunDir('browser-state', 1 * day); // must be ignored: not a timestamp dir

// session-meta: one expired port dir, one fresh.
makeRunDir('session-meta/port-9222', 10 * day);
makeRunDir('session-meta/port-9333', 1 * day);

const run = spawnSync(process.execPath, [
  'skills/octocode-chrome-devtools/scripts/prune-artifacts.mjs',
  '--base', base,
  '--max-age-days', '3',
  '--max-count', '2',
], { encoding: 'utf8', timeout: 15000 });

assert('prune script exits 0', run.status === 0, run.stderr);

let parsed = null;
try { parsed = JSON.parse(run.stdout); } catch (e) { assert('output is valid JSON', false, run.stdout); }
if (parsed) {
  assert('found 4 run dirs (browser-state excluded)', parsed.runDirs.found === 4, JSON.stringify(parsed.runDirs));
  // 1 expired by age + 1 over the max-count=2 cap among the 3 remaining fresh dirs = 2 removed, 2 kept.
  assert('removed expired + over-cap run dirs', parsed.runDirs.removed === 2, JSON.stringify(parsed.runDirs));
  assert('found 2 session-meta dirs', parsed.sessionMetaDirs.found === 2, JSON.stringify(parsed.sessionMetaDirs));
  assert('removed only the expired session-meta dir', parsed.sessionMetaDirs.removed === 1, JSON.stringify(parsed.sessionMetaDirs));
}

assert('oldest run dir removed from disk', !existsSync(join(base, '2026-01-01-00-00-00')));
assert('non-timestamp dir left untouched', existsSync(join(base, 'browser-state')));
assert('expired session-meta dir removed', !existsSync(join(base, 'session-meta', 'port-9222')));
assert('fresh session-meta dir kept', existsSync(join(base, 'session-meta', 'port-9333')));

// dry-run must not delete anything, even when everything is expired.
const base2 = mkdtempSync(join(tmpdir(), 'prune-artifacts-eval-dryrun-'));
mkdirSync(join(base2, '2020-01-01-00-00-00'), { recursive: true });
utimesSync(join(base2, '2020-01-01-00-00-00'), 0, 0);
const dryRun = spawnSync(process.execPath, [
  'skills/octocode-chrome-devtools/scripts/prune-artifacts.mjs',
  '--base', base2, '--max-age-days', '1', '--dry-run',
], { encoding: 'utf8', timeout: 15000 });
assert('dry-run exits 0', dryRun.status === 0, dryRun.stderr);
assert('dry-run does not delete', existsSync(join(base2, '2020-01-01-00-00-00')));

rmSync(base, { recursive: true, force: true });
rmSync(base2, { recursive: true, force: true });

const failed = checks.filter(c => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
