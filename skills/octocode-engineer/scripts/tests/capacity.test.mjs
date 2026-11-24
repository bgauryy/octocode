import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { fitUsl, parseRuns } from '../capacity.mjs';

const SCRIPT = fileURLToPath(new URL('../capacity.mjs', import.meta.url));
const usl = (lambda, alpha, beta) => (n) => (lambda * n) / (1 + alpha * (n - 1) + beta * n * (n - 1));
const LEVELS = [1, 2, 4, 8, 16, 32, 64, 128];
const csv = (model, levels = LEVELS, noise = () => 1) => `concurrency,throughput\n${levels.map((n) => `${n},${model(n) * noise(n)}`).join('\n')}\n`;
const close = (actual, expected, tolerance, label) => assert.ok(Math.abs(actual - expected) <= tolerance * Math.abs(expected), `${label}: ${actual} vs ${expected}`);

function cli(input, args = ['usl', '-']) {
  const run = spawnSync(process.execPath, [SCRIPT, ...args], { input, encoding: 'utf8' });
  return { status: run.status, output: JSON.parse(run.stdout) };
}

test('exact USL data recovers λ, α, β and the peak √((1 − α)/β)', () => {
  const [lambda, alpha, beta] = [1000, 0.05, 0.0002];
  const fit = fitUsl(parseRuns(csv(usl(lambda, alpha, beta))));
  close(fit.lambda, lambda, 1e-6, 'lambda');
  close(fit.alpha, alpha, 1e-6, 'alpha');
  close(fit.beta, beta, 1e-6, 'beta');
  close(fit.peak.concurrency, Math.sqrt((1 - alpha) / beta), 1e-6, 'peak');
  assert.equal(fit.peak.extrapolated, false);
  const model = usl(fit.lambda, fit.alpha, fit.beta);
  for (const n of [fit.peak.bestInteger - 1, fit.peak.bestInteger + 1]) assert.ok(model(n) <= model(fit.peak.bestInteger), 'bestInteger is the best whole concurrency');
  close(fit.fit.r2, 1, 1e-9, 'r2');
  assert.equal(fit.fit.dof, LEVELS.length - 3);
  assert.deepEqual(fit.constrained, []);
});

test('noisy data still fits, and the fit quality reports the noise', () => {
  const noise = (n) => 1 + 0.02 * Math.sin(n * 7);
  const fit = fitUsl(parseRuns(csv(usl(500, 0.1, 0.001), LEVELS, noise)));
  assert.ok(fit.fit.r2 > 0.99 && fit.fit.r2 < 1);
  assert.ok(fit.fit.maxRelativeError <= 0.05);
  for (const row of fit.rows) close(row.relativeError, (row.predicted - row.throughput) / row.throughput, 1e-12, 'relativeError');
});

test('a peak beyond the measured levels is flagged as extrapolated', () => {
  const fit = fitUsl(parseRuns(csv(usl(100, 0.05, 0.0002), [1, 2, 4, 8])));
  assert.ok(fit.peak.concurrency > 8);
  assert.equal(fit.peak.extrapolated, true);
  assert.ok(fit.warnings.some((warning) => /beyond the largest measured/.test(warning)));
});

test('exact Amdahl data fits β = 0 without a constraint: no peak, a ceiling of λ/α', () => {
  const fit = fitUsl(parseRuns(csv(usl(200, 0.1, 0))));
  assert.equal(fit.beta, 0);
  assert.deepEqual(fit.constrained, [], 'a term negative only by rounding is zero, not held');
  assert.equal(fit.peak, null);
  close(fit.ceiling, fit.lambda / fit.alpha, 1e-12, 'ceiling');
  close(fit.alpha, 0.1, 1e-6, 'alpha');
});

test('a slightly better-than-Amdahl tail holds β at 0 and says so', () => {
  const fit = fitUsl(parseRuns(csv(usl(200, 0.1, 0), LEVELS, (n) => 1 + 0.002 * Math.log2(n))));
  assert.deepEqual(fit.constrained, ['beta']);
  assert.equal(fit.peak, null);
  assert.ok(fit.warnings.some((warning) => /beta held at 0/.test(warning)));
});

test('superlinear data holds the negative terms at 0 and says so', () => {
  const fit = fitUsl(parseRuns('1,100\n2,210\n4,430\n8,900\n'));
  assert.ok(fit.constrained.length > 0);
  assert.ok(fit.alpha >= 0 && fit.beta >= 0);
  assert.ok(fit.warnings.some((warning) => /held at 0/.test(warning)));
});

test('three levels fit exactly and warn that dof 0 proves nothing', () => {
  const fit = fitUsl(parseRuns(csv(usl(100, 0.05, 0.01), [1, 4, 16])));
  assert.equal(fit.fit.dof, 0);
  assert.ok(fit.warnings.some((warning) => /dof 0/.test(warning)));
});

test('the CLI reads stdin or a file and rejects malformed input with exit 2', () => {
  const ok = cli(csv(usl(1000, 0.05, 0.0002)));
  assert.equal(ok.status, 0);
  assert.equal(ok.output.model, 'usl');
  for (const [input, code] of [['1,100\n2,200\n', 'tooFewLevels'], ['1,100\nx,200\n4,300\n', 'invalidRow'], ['0,100\n2,200\n4,300\n', 'invalidRow'], ['1,100,3\n', 'invalidRow']]) {
    const bad = cli(input);
    assert.equal(bad.status, 2, input);
    assert.equal(bad.output.error.code, code);
  }
  assert.equal(cli('', ['amdahl', '-']).output.error.code, 'unknownModel');
  assert.equal(cli('', ['usl', '/nonexistent/runs.csv']).output.error.code, 'unreadable');
  assert.equal(spawnSync(process.execPath, [SCRIPT, '--help'], { encoding: 'utf8' }).status, 0);
});
