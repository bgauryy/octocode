// The evaluate plan op reads page state by value; raw Runtime script methods need acknowledgement,
// while the engine's own internal evaluations (extract, act, snapshot) never do.
import test from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import {
  run,
  validatePlan,
} from '../../dist/engine/cdp-checks/browser-execute.mjs';
import { CDP_POLICY_TABLE } from '../../dist/engine/cdp-policy.mjs';
import { call, fixture } from './plan-fixture.mjs';

const dist = join(import.meta.dirname, '../../dist/engine');
const core = await import(join(dist, 'flows/flow-core.mjs'));
const { builtinFlows } = await import(join(dist, 'flows/index.mjs'));

/** A page whose Runtime.evaluate answers from `answers[expression]` (a value, or an Error to throw in the page). */
const withPage = (cdp, answers) => {
  const sends = cdp.send;
  cdp.send = async (method, params, sessionId) => {
    if (method !== 'Runtime.evaluate') return sends(method, params, sessionId);
    await sends(method, params, sessionId);
    const answer = answers[params.expression];
    if (answer instanceof Error)
      return {
        result: { type: 'object' },
        exceptionDetails: {
          text: 'Uncaught',
          exception: { description: `Error: ${answer.message}` },
        },
      };
    return { result: { type: 'string', value: answer } };
  };
};
const stepsOf = read => read('browser-result.json').steps;

test('evaluate records the page value in the step data and asks for by-value results', async () =>
  fixture(async ({ cdp, read, sent }) => {
    withPage(cdp, { 'document.title': 'Fixture' });
    await run(cdp, {
      steps: [{ op: 'evaluate', expression: 'document.title' }],
    });
    const [step] = stepsOf(read);
    assert.equal(step.status, 'complete');
    assert.equal(step.data.result, 'Fixture');
    assert.deepEqual(sent[0].params, {
      expression: 'document.title',
      returnByValue: true,
      awaitPromise: false,
    });
  }));

test('awaitPromise is passed through so a promise resolves to its value', async () =>
  fixture(async ({ cdp, read, sent }) => {
    withPage(cdp, { 'Promise.resolve(7)': 7 });
    await run(cdp, {
      steps: [
        {
          op: 'evaluate',
          expression: 'Promise.resolve(7)',
          awaitPromise: true,
        },
      ],
    });
    assert.equal(stepsOf(read)[0].data.result, 7);
    assert.equal(sent[0].params.awaitPromise, true);
  }));

test('an exception fails the step with its message; optional failure continues', async () => {
  await fixture(async ({ cdp, read }) => {
    withPage(cdp, { 'boom()': new Error('boom happened') });
    await run(cdp, { steps: [{ op: 'evaluate', expression: 'boom()' }] });
    const result = read('browser-result.json');
    assert.equal(result.ok, false);
    assert.match(result.steps[0].error, /boom happened/);
    assert.equal(result.steps[0].data, undefined);
  });
  await fixture(async ({ cdp, read }) => {
    withPage(cdp, { 'boom()': new Error('soft'), 'document.title': 'After' });
    await run(cdp, {
      steps: [
        { op: 'evaluate', expression: 'boom()', optional: true },
        { op: 'evaluate', expression: 'document.title' },
      ],
    });
    const result = read('browser-result.json');
    assert.equal(result.ok, true);
    assert.equal(result.steps[0].status, 'failed');
    assert.equal(result.steps[1].data.result, 'After');
    assert.equal(result.failedOptionalSteps.length, 1);
  });
  process.exitCode = undefined;
});

test('evaluate honors after: the step then waits for its condition', async () =>
  fixture(async ({ cdp, read }) => {
    withPage(cdp, { '1+1': 2 });
    const answer = cdp.send;
    cdp.send = async (method, params, sessionId) =>
      params?.expression === '1+1'
        ? answer(method, params, sessionId)
        : { result: { value: { ready: true, matched: true, url: 'u' } } };
    await run(cdp, {
      steps: [
        { op: 'evaluate', expression: '1+1', after: { selector: 'main' } },
      ],
    });
    const [step] = stepsOf(read);
    assert.equal(step.status, 'complete');
    assert.equal(step.data.result, 2);
    assert.equal(step.condition.matched, true);
  }));

test('evaluate works inside a spliced flow plan', async () =>
  fixture(async ({ cdp, read, emit }) => {
    const flow = builtinFlows.find(item => item.name === 'handle-dialog');
    const { values } = core.resolveParams(flow, { trigger: 'alert(1)' });
    const { plan } = core.expandFlow(flow, values);
    assert.ok(plan.steps.some(step => step.op === 'evaluate'));
    const sends = cdp.send;
    cdp.send = async (method, params, sessionId) => {
      const out = await sends(method, params, sessionId);
      if (method === 'Runtime.evaluate')
        queueMicrotask(() =>
          emit('Page.javascriptDialogOpening', { message: 'hi' })
        );
      return method === 'Runtime.evaluate' ? { result: { value: null } } : out;
    };
    await run(cdp, {
      steps: [...plan.steps, { op: 'evaluate', expression: '40+2' }],
    });
    const result = read('browser-result.json');
    assert.equal(result.ok, true, JSON.stringify(result.failure));
    assert.equal(result.steps.length, plan.steps.length + 1);
  }));

test('raw script methods through the cdp op need acknowledge; the evaluate op does not', () => {
  const scripts = [
    'Runtime.evaluate',
    'Runtime.callFunctionOn',
    'Runtime.compileScript',
    'Runtime.runScript',
  ];
  for (const method of scripts) {
    const entry = CDP_POLICY_TABLE.find(row => row.method === method);
    assert.equal(entry.policy, 'needsAck');
    assert.match(entry.reason, /evaluate plan op/);
    assert.throws(
      () => validatePlan({ steps: [{ op: 'cdp', method, params: {} }] }),
      /acknowledge: true/
    );
    validatePlan({
      steps: [{ op: 'cdp', method, params: {}, acknowledge: true }],
    });
  }
  validatePlan({ steps: [{ op: 'evaluate', expression: '1' }] });
});

test('the engine internal evaluations run without acknowledge', async () =>
  fixture(async ({ cdp, read }) => {
    const sends = cdp.send;
    cdp.send = async (method, params, sessionId) => {
      await sends(method, params, sessionId);
      return method === 'Runtime.evaluate'
        ? {
            result: {
              value: {
                rows: [{ text: 'a', href: null }],
                url: 'u',
                title: 't',
              },
            },
          }
        : {};
    };
    const plan = {
      steps: [{ op: 'extract', selector: 'a' }, call('Runtime.enable')],
    };
    validatePlan(plan);
    await run(cdp, plan);
    const result = read('browser-result.json');
    assert.equal(result.ok, true, JSON.stringify(result.failure));
    assert.equal(result.steps[0].count, 1);
    // act, wait, snapshot and screenshot plans validate with no acknowledge anywhere.
    validatePlan({
      steps: [
        { op: 'act', action: 'click', selector: '#b' },
        { op: 'wait', selector: '#b' },
        { op: 'snapshot' },
        { op: 'screenshot' },
      ],
    });
  }));
