import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tempWorkspace } from '../helpers.mjs';

const script = fileURLToPath(new URL('../../src/compare-service.mjs', import.meta.url));
// These are grader fixtures, never benchmark observations.
function fixture(t) {
  const dir = tempWorkspace(t, 'communication-comparison-test-');
  const rows = [0, 1, 2, 3].map(i => ({ passed: true, modelCalls: 0, lostMessages: 0, duplicates: 0,
    childExit: { code: 0, signal: null }, manifest: { harnessSha256: 'harness', binarySha256: i % 2 ? 'candidate' : 'baseline', binary: '/same/path', samples: 60, startedAt: new Date(1000 * [1, 2, 4, 3][i]).toISOString() },
    metrics: { mcpSend: { n: 60, p50Ms: i % 2 ? 1 : 5, p95Ms: i % 2 ? 2 : 6 }, cliSend: { n: 60, p50Ms: 10, p95Ms: 12 } } }));
  return { rows, run() {
    const paths = rows.map((row, i) => { const path = join(dir, `${i}.json`); writeFileSync(path, JSON.stringify(row)); return path; });
    const output = join(dir, 'comparison.json');
    execFileSync(process.execPath, [script, ...paths], { env: { ...process.env, COMMUNICATION_OUTPUT: output }, stdio: 'pipe' });
    return JSON.parse(readFileSync(output));
  } };
}
test('service comparison enforces frozen primary and latency gates', t => {
  const f = fixture(t); assert.equal(f.run().verdict, 'KEEP_EXPLORATORY');
  f.rows[1].metrics.mcpSend.p50Ms = 4.01;
  assert.throws(() => f.run());
  f.rows[1].metrics.mcpSend.p50Ms = 4;
  assert.equal(f.run().passed, true);
  f.rows[1].metrics.cliSend.p95Ms = 20.01; // 12 * 1.25 + 5 = 20.
  assert.throws(() => f.run());
});
test('service comparison rejects missing telemetry and mismatched subjects', async t => {
  const mutations = [
    rows => { delete rows[1].modelCalls; }, rows => { rows[1].childExit.signal = 'SIGTERM'; },
    rows => { rows[1].manifest.harnessSha256 = 'changed'; }, rows => { rows[1].manifest.binary = '/other/path'; },
    rows => { rows[3].manifest.binarySha256 = 'another-candidate'; }, rows => { rows[1].metrics.mcpSend.p50Ms = null; },
    rows => { rows[3].manifest.startedAt = new Date(5000).toISOString(); },
    rows => { for (const row of rows) delete row.manifest.harnessSha256; },
    rows => { rows[1].manifest.startedAt = 'unknown'; },
  ];
  for (let i = 0; i < mutations.length; i++) await t.test(`invalid fixture ${i}`, t => { const f = fixture(t); mutations[i](f.rows); assert.throws(() => f.run()); });
});
