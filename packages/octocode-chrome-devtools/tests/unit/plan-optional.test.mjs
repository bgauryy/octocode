import test from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import {
  validatePlan,
  run,
} from '../../dist/engine/cdp-checks/browser-execute.mjs';
import { chromePlanOperations } from '../../dist/engine/chrome-contract.mjs';
import { call, fixture } from './plan-fixture.mjs';
import { shape } from './capture-fixture.mjs';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

// A connection whose named methods throw; every other command succeeds.
const failing = (cdp, methods) => {
  const sent = [];
  cdp.send = async method => {
    sent.push(method);
    if (methods.includes(method)) throw new Error(`${method} refused`);
    return {};
  };
  return sent;
};

test('a failing optional step is recorded and later steps still run', async () =>
  fixture(async ({ cdp, read }) => {
    const sent = failing(cdp, ['Runtime.evaluate']);
    const result = await run(cdp, {
      steps: [
        call('Runtime.enable'),
        call('Runtime.evaluate', {}, { optional: true }),
        call('Runtime.disable'),
      ],
    });
    assert.deepEqual(sent, [
      'Runtime.enable',
      'Runtime.evaluate',
      'Runtime.disable',
    ]);
    const saved = read('browser-result.json');
    assert.equal(saved.ok, true);
    assert.equal(saved.failure, null);
    assert.equal(result.ok, true);
    assert.deepEqual(
      saved.steps.map(({ index, status, optional }) => [
        index,
        status,
        optional ?? false,
      ]),
      [
        [1, 'complete', false],
        [2, 'failed', true],
        [3, 'complete', false],
      ]
    );
    assert.equal(saved.steps[1].error, 'Runtime.evaluate refused');
    assert.equal(saved.completedSteps, 2);
    assert.equal(saved.requestedSteps, 3);
    assert.deepEqual(
      saved.failedOptionalSteps.map(({ step, error }) => [step, error]),
      [[2, 'Runtime.evaluate refused']]
    );
  }));

test('an optional failure leaves the process exit status as it was', async () =>
  fixture(async ({ cdp }) => {
    const before = process.exitCode;
    cdp.send = async () => {
      process.exitCode = 1; // what a DOM recipe does when its action fails
      throw new Error('action failed');
    };
    await run(cdp, { steps: [call('Runtime.enable', {}, { optional: true })] });
    assert.equal(process.exitCode, before);
  }));

test('a failing required step still stops the plan, and earlier optional failures stay listed', async () =>
  fixture(async ({ cdp, read }) => {
    const sent = failing(cdp, ['Runtime.evaluate', 'Runtime.disable']);
    const previous = process.exitCode;
    await run(cdp, {
      steps: [
        call('Runtime.evaluate', {}, { optional: true }),
        call('Runtime.disable'),
        call('Runtime.enable'),
      ],
    });
    assert.equal(process.exitCode, 1);
    process.exitCode = previous;
    assert.deepEqual(sent, ['Runtime.evaluate', 'Runtime.disable']);
    const saved = read('browser-result.json');
    assert.equal(saved.ok, false);
    assert.equal(saved.failure.step, 2);
    assert.equal(saved.failedOptionalSteps.length, 1);
    assert.equal(saved.coverage.unvisitedSteps, 1);
  }));

test('a plan without optional failures reports none', async () =>
  fixture(async ({ cdp, read }) => {
    await run(cdp, { steps: [call('Runtime.enable', {}, { optional: true })] });
    assert.deepEqual(read('browser-result.json').failedOptionalSteps, []);
  }));

test('a failed optional step does not evaluate its after postcondition', async () =>
  fixture(async ({ cdp, read }) => {
    const sent = failing(cdp, ['Page.enable']);
    await run(cdp, {
      steps: [
        {
          op: 'goto',
          url: 'https://fixture.invalid/',
          optional: true,
          after: { selector: 'main' },
        },
        call('Runtime.enable'),
      ],
    });
    assert.deepEqual(sent, ['Page.enable', 'Runtime.enable']);
    assert.equal(read('browser-result.json').steps[0].condition, undefined);
  }));

test('optional is accepted on every operation and rejected unless boolean, before any connection', () => {
  const samples = {
    goto: { url: 'https://example.com' },
    history: { direction: 'back' },
    reload: {},
    act: { action: 'click', selector: 'a' },
    wait: { selector: 'a' },
    extract: { selector: 'a' },
    evaluate: { expression: 'document.title' },
    cdp: { method: 'Runtime.enable' },
    protocol: { domain: 'Runtime' },
    listen: { id: 'l', event: 'Page.loadEventFired' },
    waitEvent: { listener: 'l' },
    readStream: { handle: 'h' },
    snapshot: {},
    screenshot: {},
  };
  assert.deepEqual(
    Object.keys(samples).sort(),
    Object.keys(chromePlanOperations).sort()
  );
  for (const [op, fields] of Object.entries(samples)) {
    const lead =
      op === 'waitEvent'
        ? [
            samples.listen && {
              op: 'listen',
              id: 'l',
              event: 'Page.loadEventFired',
            },
          ]
        : [];
    const step = { op, ...fields };
    assert.doesNotThrow(() =>
      validatePlan({ steps: [...lead, { ...step, optional: true }] })
    );
    for (const optional of ['yes', 1, null, {}])
      assert.throws(
        () => validatePlan({ steps: [...lead, { ...step, optional }] }),
        /optional must be boolean/,
        `${op} ${JSON.stringify(optional)}`
      );
  }
});

test('snapshot and screenshot steps allow no after, frame or extra fields, and one of each per plan', () => {
  for (const op of ['snapshot', 'screenshot']) {
    assert.doesNotThrow(() => validatePlan({ steps: [{ op }] }));
    assert.doesNotThrow(() =>
      validatePlan({ steps: [{ op, session: 'flat', timeoutMs: 100 }] })
    );
    for (const extra of [
      { after: { text: 'x' } },
      { frame: { id: 'f' } },
      { selector: 'a' },
    ])
      assert.throws(
        () => validatePlan({ steps: [{ op, ...extra }] }),
        /Unknown/
      );
    assert.throws(
      () => validatePlan({ steps: [{ op }, call('Runtime.enable'), { op }] }),
      new RegExp(`one ${op} step`)
    );
  }
  assert.doesNotThrow(() =>
    validatePlan({ steps: [{ op: 'snapshot' }, { op: 'screenshot' }] })
  );
});

test('a screenshot step reuses the screenshot recipe and yields its image artifact', async () =>
  fixture(async ({ cdp, read }) => {
    const sent = [];
    cdp.send = async method => {
      sent.push(method);
      if (method === 'Runtime.evaluate')
        return { result: { value: 'complete' } };
      if (method === 'Page.captureScreenshot')
        return { data: Buffer.from('image-bytes').toString('base64') };
      return {};
    };
    await run(cdp, { steps: [call('Runtime.enable'), { op: 'screenshot' }] });
    const saved = read('browser-result.json');
    assert.equal(saved.ok, true);
    const row = saved.steps[1];
    assert.equal(row.status, 'complete');
    assert.equal(row.count, 1);
    assert.ok(existsSync(row.artifact), row.artifact);
    assert.ok(sent.includes('Page.captureScreenshot'));
  }));

test('a failed optional screenshot does not stop a later step', async () =>
  fixture(async ({ cdp, read }) => {
    cdp.send = async method => {
      if (method === 'Page.captureScreenshot') throw new Error('no surface');
      if (method === 'Runtime.evaluate')
        return { result: { value: 'complete' } };
      return {};
    };
    await run(cdp, {
      steps: [{ op: 'screenshot', optional: true }, call('Runtime.enable')],
    });
    const saved = read('browser-result.json');
    assert.equal(saved.ok, true);
    assert.equal(saved.failedOptionalSteps[0].step, 1);
    assert.equal(saved.steps[1].status, 'complete');
  }));

test('the flow page lists failed optional steps and keeps the plan successful', async () => {
  const work = mkdtempSync(join(tmpdir(), 'octo-optional-flow-'));
  try {
    const flow = join(work, 'browser-result.json');
    writeFileSync(
      flow,
      JSON.stringify({
        ok: true,
        failure: null,
        requestedSteps: 2,
        completedSteps: 1,
        failedOptionalSteps: [{ step: 1, error: 'no such control' }],
        steps: [
          {
            index: 1,
            op: 'act',
            optional: true,
            status: 'failed',
            error: 'no such control',
          },
          {
            index: 2,
            op: 'cdp',
            status: 'complete',
            condition: { ready: true },
          },
        ],
        eventCoverage: [],
      })
    );
    const result = await shape(work, `[ARTIFACT] browser-result.json ${flow}`);
    assert.equal(result.ok, true);
    assert.equal(result.flow.failure, undefined);
    assert.deepEqual(result.flow.failedOptionalSteps, [
      { step: 1, error: 'no such control' },
    ]);
    assert.equal(result.flow.steps[0].optional, true);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
