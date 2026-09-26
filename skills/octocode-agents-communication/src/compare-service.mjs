import assert from 'node:assert/strict';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

// Pairs are baseline/candidate paths. Run both AB and BA before drawing a conclusion.
const files = process.argv.slice(2);
assert.ok(files.length >= 4 && files.length % 2 === 0, 'Supply at least two baseline/candidate result pairs (AB and BA order).');
const rows = [];
let harness, baselineHash, candidateHash;
for (let i = 0; i < files.length; i += 2) {
  const [baseline, candidate] = files.slice(i, i + 2).map(path => JSON.parse(readFileSync(path)));
  for (const result of [baseline, candidate]) {
    assert.equal(result.passed, true, 'Single-run correctness must pass');
    for (const field of ['modelCalls', 'lostMessages', 'duplicates']) assert.equal(result[field], 0, `Missing/nonzero ${field}`);
    assert.equal(result.childExit.code, 0);
    assert.equal(result.childExit.signal, null);
    for (const field of ['harnessSha256', 'binarySha256', 'binary']) assert.ok(typeof result.manifest[field] === 'string' && result.manifest[field].length > 0, `Missing ${field}`);
    assert.ok(Number.isFinite(Date.parse(result.manifest.startedAt)), 'Invalid run timestamp');
    assert.equal(result.manifest.harnessSha256, harness ??= result.manifest.harnessSha256);
    for (const name of ['cliSend', 'mcpSend']) for (const metric of ['p50Ms', 'p95Ms']) assert.ok(Number.isFinite(result.metrics[name][metric]) && result.metrics[name][metric] > 0, `Invalid ${name}.${metric}`);
    assert.ok(result.metrics.mcpSend.n >= 10);
  }
  assert.equal(baseline.manifest.binarySha256, baselineHash ??= baseline.manifest.binarySha256);
  assert.equal(candidate.manifest.binarySha256, candidateHash ??= candidate.manifest.binarySha256);
  assert.notEqual(baselineHash, candidateHash, 'Need different subjects');
  assert.equal(baseline.manifest.binary, candidate.manifest.binary, 'Use the same executable path to control host launch effects');
  assert.equal(baseline.manifest.samples, candidate.manifest.samples);
  const mcpRatio = candidate.metrics.mcpSend.p50Ms / baseline.metrics.mcpSend.p50Ms;
  const cliLimit = baseline.metrics.cliSend.p95Ms * 1.25 + 5;
  rows.push({ baseline: resolve(files[i]), candidate: resolve(files[i + 1]), order: Date.parse(baseline.manifest.startedAt) < Date.parse(candidate.manifest.startedAt) ? 'AB' : 'BA',
    baselineMcpP50Ms: baseline.metrics.mcpSend.p50Ms, candidateMcpP50Ms: candidate.metrics.mcpSend.p50Ms,
    mcpReductionPercent: (1 - mcpRatio) * 100, primaryPass: mcpRatio <= .8,
    baselineCliP95Ms: baseline.metrics.cliSend.p95Ms, candidateCliP95Ms: candidate.metrics.cliSend.p95Ms,
    cliP95LimitMs: cliLimit, cliGuardPass: candidate.metrics.cliSend.p95Ms <= cliLimit });
}
assert.ok(rows.some(row => row.order === 'AB') && rows.some(row => row.order === 'BA'), 'Balance whole-run order');
const passed = rows.every(row => row.primaryPass && row.cliGuardPass);
const result = { verdict: passed ? 'KEEP_EXPLORATORY' : 'REVISE', passed, rows, harnessSha256: harness, baselineHash, candidateHash,
  limits: 'Local microbenchmark of the complete version change, not isolated attribution or statistical evidence. Provider inference/cache performance is outside this sensor.' };
const output = resolve(process.env.COMMUNICATION_OUTPUT ?? 'service-comparison.json');
mkdirSync(dirname(output), { recursive: true }); writeFileSync(output, JSON.stringify(result, null, 2));
console.log(JSON.stringify({ output, ...result }, null, 2));
if (!passed) process.exitCode = 1;
