import test from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { join } from 'node:path';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';

const { outputFiles } = await build({
  entryPoints: [join(import.meta.dirname, '../../src/invocation.ts')],
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
});
const dir = mkdtempSync(join(tmpdir(), 'octo-failure-'));
const file = join(dir, 'invocation.mjs');
writeFileSync(file, outputFiles[0].text);
const { failureSummary } = await import(pathToFileURL(file).href);
rmSync(dir, { recursive: true, force: true });
const next = { capture: { tool: 'artifact', query: { file: 'c.json' } } };

test('the failed plan step is the cause, with its step number and the next call', () => {
  const text = failureSummary({
    exitCode: 1,
    flow: { failure: { step: 2, error: '0 elements match role/name' } },
    next,
  });
  assert.match(
    text,
    /^Failed: step 2: 0 elements match role\/name; next: artifact /
  );
  assert.ok(text.includes('c.json'));
});

test('a stderr error outranks the plan failure, and exit status is the last resort', () => {
  assert.match(
    failureSummary({ error: 'boom\nmore', flow: { failure: { error: 'x' } } }),
    /^Failed: boom/
  );
  assert.equal(failureSummary({ exitCode: 3 }), 'Failed: exit 3');
  assert.equal(failureSummary({ timedOut: true }), 'Failed: timed out');
});

test('optional steps that failed before the stop are listed with the cause', () => {
  const text = failureSummary({
    exitCode: 1,
    flow: {
      failure: { step: 3, error: 'required control missing' },
      failedOptionalSteps: [{ step: 1, error: 'banner absent' }],
    },
  });
  assert.match(text, /^Failed: step 3: required control missing/);
  assert.match(text, /optional steps failed earlier: step 1: banner absent/);
});
