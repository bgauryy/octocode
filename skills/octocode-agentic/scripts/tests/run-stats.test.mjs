import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { percentile } from '../run-stats.mjs';

const script = new URL('../run-stats.mjs', import.meta.url).pathname;
const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });

function runsFile(dir, rows, name = 'runs.jsonl') {
  const file = join(dir, name);
  writeFileSync(file, `${rows.map((row) => JSON.stringify(row)).join('\n')}\n`);
  return file;
}

test('percentile interpolates between ranks', () => {
  assert.equal(percentile([4, 1, 3, 2], 0.5), 2.5);
  assert.equal(percentile([1, 2, 3, 4], 0.95), 1 + 3 * 0.95);
  assert.equal(percentile([], 0.5), null);
});

test('per-arm summary counts unknown runs as non-success and keeps token buckets apart', () => {
  const dir = mkdtempSync(join(tmpdir(), 'run-stats-'));
  try {
    const rows = [
      { task: 't1', arm: 'A', success: true, costUsd: 0.2, latencyMs: 100, tokens: { input: 10, cacheRead: 30, cacheWrite: 10, output: 5 } },
      { task: 't2', arm: 'A', success: false, costUsd: 0.1, latencyMs: 300 },
      { task: 't3', arm: 'A', success: null, costUsd: 0.1 },
      { task: 't1', arm: 'B', success: true, latencyMs: 50 },
    ];
    const result = run(runsFile(dir, rows));
    assert.equal(result.status, 0, result.stdout);
    const { A, B } = JSON.parse(result.stdout).arms;
    assert.equal(A.successRate, Number((A.successes / A.runs).toFixed(6)));
    assert.equal(A.unknown, 1);
    assert.equal(A.costPerSuccess, Number((A.costUsd / A.successes).toFixed(6)));
    assert.deepEqual(A.latencyMs, { n: 2, p50: 200, p95: 290 });
    assert.deepEqual(A.tokens, { input: 10, cacheRead: 30, cacheWrite: 10, output: 5 });
    assert.equal(A.cacheHitRate, 30 / 50);
    assert.equal(B.costPerSuccess, null, 'a run without costUsd leaves cost per success unknown');
    assert.equal(B.cacheHitRate, null);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('paired deltas use only shared tasks, are reproducible by seed and bracket a uniform gain', () => {
  const dir = mkdtempSync(join(tmpdir(), 'run-stats-'));
  try {
    const rows = [];
    for (let t = 0; t < 20; t++) {
      for (let r = 0; r < 2; r++) {
        rows.push({ task: `t${t}`, arm: 'base', success: t % 2 === 0, costUsd: 1, latencyMs: 100 + t });
        rows.push({ task: `t${t}`, arm: 'cand', success: true, costUsd: 1, latencyMs: 100 + t });
      }
    }
    rows.push({ task: 'solo', arm: 'cand', success: true, costUsd: 1 });
    const file = runsFile(dir, rows);
    const args = [file, '--baseline', 'base', '--candidate', 'cand', '--resamples', '500', '--seed', '7'];
    const first = run(...args);
    assert.equal(first.status, 0, first.stdout);
    assert.equal(first.stdout, run(...args).stdout);
    const { paired } = JSON.parse(first.stdout);
    assert.equal(paired.tasks, 20);
    assert.deepEqual(paired.unpaired, ['solo']);
    const success = paired.deltas.successRate;
    assert.equal(success.estimate, Number((success.candidate - success.baseline).toFixed(6)));
    assert.ok(success.low > 0 && success.low <= success.estimate && success.estimate <= success.high);
    assert.deepEqual([paired.deltas.latencyP50.low, paired.deltas.latencyP50.high], [0, 0]);
    assert.ok(paired.deltas.costPerSuccess.high < 0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('invalid input exits 2 with a JSON error', () => {
  const dir = mkdtempSync(join(tmpdir(), 'run-stats-'));
  try {
    const good = runsFile(dir, [{ task: 't1', arm: 'A', success: true }]);
    const bad = runsFile(dir, [{ task: 't1', arm: 'A', success: 'yes' }], 'bad.jsonl');
    assert.equal(run(bad).status, 2);
    assert.equal(JSON.parse(run(bad).stdout).ok, false);
    assert.equal(run(good, '--baseline', 'A').status, 2);
    assert.equal(run(good, '--baseline', 'A', '--candidate', 'Z').status, 2);
    assert.equal(run(good, '--level', '2').status, 2);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
