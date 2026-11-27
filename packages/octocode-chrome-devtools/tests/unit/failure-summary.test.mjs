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
const { failureSummary, failurePayload } = await import(
  pathToFileURL(file).href
);
rmSync(dir, { recursive: true, force: true });
const next = { capture: { tool: 'query', query: { file: 'c.json' } } };

test('the failed plan step is the cause, with its step number and the next call', () => {
  const text = failureSummary({
    exitCode: 1,
    flow: { failure: { step: 2, error: '0 elements match role/name' } },
    next,
  });
  assert.match(
    text,
    /^Failed: step 2: 0 elements match role\/name; next: query /
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

test('the cause skips the sandbox banner and Node warnings and names the fatal line', () => {
  const error = [
    '[CDP_SANDBOX] sandboxed (node 26.4.0, fs scoped, net/dns loopback-only) — rerun with --verbose for full detail',
    '(node:8662) ExperimentalWarning: The flag --allow-net is under experimental phase.',
    '(Use `node --trace-warnings ...` to show where the warning was created)',
    '[CDP_RUNNER_FATAL] Ambiguous target: 3 matches. Run targets and select --target <id>.',
  ].join('\n');
  assert.match(
    failureSummary({ error, exitCode: 1 }),
    /^Failed: \[CDP_RUNNER_FATAL\] Ambiguous target: 3 matches/
  );
  assert.match(
    failureSummary({ error: 'only noise\n(node:1) ExperimentalWarning: x' }),
    /^Failed: only noise/
  );
});

const envelope = {
  ok: false,
  exitCode: 2,
  error: '[CDP_RUNNER_FATAL] Refusing Browser.close',
  capture: { file: 'c.json', artifacts: 0, findings: 0 },
  logs: { stdout: 'o.txt', stderr: 'e.txt' },
  search: { paths: ['p'] },
  artifacts: [{ label: 'x', path: 'y' }],
  root: '/r',
  next: {
    capture: { tool: 'query', query: { file: 'c.json' } },
    steps: { tool: 'query', query: { file: 's.json' } },
  },
};

test('a failure payload keeps status, cause and one continuation, and drops the capture envelope', () => {
  const payload = failurePayload({ ...envelope, timedOut: true });
  assert.equal(Object.keys(payload)[0], 'summary');
  assert.equal(payload.ok, false);
  assert.equal(payload.exitCode, 2);
  assert.equal(payload.timedOut, true);
  assert.match(payload.error, /Refusing Browser.close/);
  assert.deepEqual(payload.next, envelope.next.capture);
  for (const dropped of [
    'capture',
    'logs',
    'search',
    'artifacts',
    'root',
    'flow',
  ])
    assert.equal(Object.hasOwn(payload, dropped), false, dropped);
  assert.deepEqual(
    failurePayload({
      ...envelope,
      next: { stderr: envelope.next.steps, ...envelope.next },
    }).next,
    envelope.next.steps
  );
});

test('a plan failure keeps its flow summary; a clean failure omits false status flags', () => {
  const flow = { failure: { step: 2, error: 'no match' } };
  assert.deepEqual(failurePayload({ ...envelope, flow }).flow, flow);
  const plain = failurePayload(envelope);
  for (const flag of ['timedOut', 'cancelled', 'outcomeUncertain'])
    assert.equal(Object.hasOwn(plain, flag), false, flag);
});

test('a refusal an engine prints as JSON on stdout is the cause, not a bare exit code', () => {
  assert.match(
    failureSummary({
      exitCode: 1,
      data: {
        status: 'ERROR',
        message:
          'A browser this package did not launch is listening on port 9480.',
      },
    }),
    /^Failed: A browser this package did not launch is listening on port 9480\./
  );
  // stderr and a failed plan step still outrank it
  assert.match(
    failureSummary({ error: 'boom', data: { message: 'other' } }),
    /^Failed: boom/
  );
  assert.equal(
    failureSummary({ exitCode: 1, data: { status: 'OK' } }),
    'Failed: exit 1'
  );
});

test('the runner Using-tab banner never wins over a later fatal or coded finding', () => {
  const banner = '[CDP_RUNNER] Using tab: https://example.test/ on port 9222';
  assert.match(
    failureSummary({
      error: [banner, '[FINDING] SOURCEMAP_POSITION_INVALID line 3'].join('\n'),
    }),
    /^Failed: \[FINDING\] SOURCEMAP_POSITION_INVALID/
  );
  assert.match(
    failureSummary({
      error: [
        banner,
        'noise',
        '[CDP_RUNNER_FATAL] Refusing Browser.close',
      ].join('\n'),
    }),
    /^Failed: \[CDP_RUNNER_FATAL\] Refusing/
  );
  // a coded finding outranks a generic marked line
  assert.match(
    failureSummary({
      error: ['Error: failed', '[FINDING] SOURCEMAP_NOT_FOUND x'].join('\n'),
    }),
    /^Failed: \[FINDING\] SOURCEMAP_NOT_FOUND/
  );
});

test('stderr with only a banner yields to a coded finding in the stdout or summary', () => {
  const error = '[CDP_RUNNER] Using tab: https://example.test/';
  for (const carrier of [
    { stdout: '[METRIC] x=1\n[FINDING] SOURCEMAP_POSITION_INVALID at 9:9' },
    { summary: ['[FINDING] SOURCEMAP_POSITION_INVALID at 9:9'] },
  ]) {
    assert.match(
      failureSummary({ error, exitCode: 1, ...carrier }),
      /^Failed: \[FINDING\] SOURCEMAP_POSITION_INVALID/
    );
    assert.match(
      failureSummary({ exitCode: 1, ...carrier }),
      /^Failed: \[FINDING\] SOURCEMAP_POSITION_INVALID/
    );
  }
});
