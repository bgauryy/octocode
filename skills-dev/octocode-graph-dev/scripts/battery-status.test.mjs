import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { batteryFailures, callFailure } from './lib/battery-status.mjs';
import { workflow } from './graph-bench.mjs';
import { compare } from './bench-compare.mjs';

test('successful empty results pass; process, parse and contract errors remain failures', () => {
  assert.equal(callFailure({ code: 1 }, { rows: [] }), null);
  assert.equal(callFailure({ code: 0 }, { rows: [{}] }), null);
  assert.deepEqual(callFailure({ code: 5 }, { errorCode: 'outputContractViolation', error: 'summary.testFunctions' }),
    { exit: 5, errorCode: 'outputContractViolation', error: 'summary.testFunctions' });
  assert.ok(callFailure({ code: 0 }, { parseError: 'invalid JSON' }));
  assert.ok(callFailure({ code: 0 }, { errorCode: 'outputContractViolation' }));
  assert.ok(callFailure({ code: null, err: 'killed' }, {}));
});

test('impact retains the actual error and the battery gate fails with its input', async () => {
  const error = { errorCode: 'outputContractViolation', error: 'summary.testFunctions' };
  const rows = await workflow(async query => ({
    res: { code: query.operation === 'impact' ? 5 : 1, out: '{}', ms: 1 },
    j: query.operation === 'impact' ? error : { rows: [] },
  }), 'file#target');
  assert.equal(rows.impact.error, error.error);
  assert.equal(rows.impact.errorCode, error.errorCode);
  const input = { queries: [{ operation: 'impact', target: 'file#target' }] };
  const failures = batteryFailures([{ lang: 'go', ingest: { exit: 0 }, failures: [{ tool: 'graphQuery', input, exit: 5, ...error }] }]);
  assert.equal(failures.length, 1);
  assert.deepEqual(failures[0].input, input);
  assert.equal(batteryFailures([{ lang: 'go', ingest: { exit: 0 }, callers: { pagesComplete: true } }]).length, 0);
  assert.equal(batteryFailures([{ lang: 'go', ingest: { exit: 0 }, callers: { pagesComplete: false } }]).length, 1);
  assert.equal(batteryFailures([{ lang: 'go', ingest: { exit: 0 }, impact: { exit: 5 } }]).length, 1);
});

const row = impact => ({ lang: 'go', ingest: { exit: 0 }, impact });

test('comparison reports all workflow status changes and fails new errors or missing stages', () => {
  const [regression] = compare([row({ exit: 0 })], [row({ exit: 5, errorCode: 'outputContractViolation' })]);
  assert.equal(regression.regression, true);
  assert.ok(regression.notes.includes('workflow failed'));
  const [fixed] = compare([row({ exit: 5 })], [row({ exit: 0 })]);
  assert.equal(fixed.regression, false);
  assert.ok(fixed.notes.includes('impact: 5 → 0'));
  const [missing] = compare([row({ exit: 0 })], [row(undefined)]);
  assert.equal(missing.regression, true);
});

test('graph battery commands run through an installed skill symlink', async t => {
  const root = mkdtempSync(join(tmpdir(), 'graph-skill-install-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const installed = join(root, 'installed');
  symlinkSync(fileURLToPath(new URL('../', import.meta.url)), installed, 'junction');
  const run = (script, ...args) => spawnSync(process.execPath, [join(installed, 'scripts', script), ...args], { encoding: 'utf8' });

  await t.test('runner prints its usage', () => {
    const result = run('graph-bench.mjs', '--help');
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Usage: graph-bench/);
  });

  await t.test('comparison reports a new failure and exits nonzero', () => {
    const before = join(root, 'before.json');
    const after = join(root, 'after.json');
    writeFileSync(before, JSON.stringify([row({ exit: 0 })]));
    writeFileSync(after, JSON.stringify([row({ exit: 5 })]));
    const result = run('bench-compare.mjs', before, after);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stdout, /workflow failed/);
  });
});
