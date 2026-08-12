#!/usr/bin/env node
/**
 * Run all hermetic CDP evals. Optional network smoke: OCTOCODE_LIVE_BENCH=1 runs eval-affiliates-stealth.mjs.
 */
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const skillRoot = resolve(fileURLToPath(new URL('.', import.meta.url)), '..');
const live = process.env.OCTOCODE_LIVE_BENCH === '1';
const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail: String(detail).slice(0, 1500) });

const HERMETIC = [
  'scripts/eval-undercover.mjs',
  'scripts/eval-actionability.mjs',
  'scripts/eval-actionability-diagnostics.mjs',
  'scripts/eval-storage-cookies.mjs',
  'scripts/eval-storage-measure.mjs',
  'scripts/eval-network-har-fetch.mjs',
  'scripts/eval-network-measure.mjs',
  'scripts/eval-performance-measure.mjs',
  'scripts/eval-measure-query.mjs',
  'scripts/eval-scrape-graph-handoff.mjs',
  'scripts/eval-prune-artifacts.mjs',
  'scripts/eval-page-snapshot.mjs',
  'scripts/eval-page-readiness.mjs',
];

function runScript(rel) {
  const path = join(skillRoot, rel);
  const res = spawnSync(process.execPath, [path], {
    cwd: resolve(process.cwd()),
    encoding: 'utf8',
    timeout: 180000,
    maxBuffer: 8 * 1024 * 1024,
  });
  assert(`hermetic: ${rel}`, res.status === 0, res.stderr || res.stdout);
}

for (const rel of HERMETIC) runScript(rel);

if (live) {
  runScript('scripts/eval-affiliates-stealth.mjs');
} else {
  assert('live_smoke', true, 'skipped (set OCTOCODE_LIVE_BENCH=1 for eval-affiliates-stealth.mjs)');
}

const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, live, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
