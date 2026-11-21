import test from 'node:test';
import assert from 'node:assert/strict';
import { run } from '../../dist/engine/cdp-checks/browser-execute.mjs';
import { fixture } from './plan-fixture.mjs';

const failureOf = async (cdp, read, plan) => {
  await run(cdp, plan);
  return read('browser-result.json').failure;
};

// A page whose action target reports the given read-back state; the wanted text never appears.
const pageReporting = (cdp, state) => {
  const target = {
    found: true,
    visible: true,
    canOperate: true,
    location: 'https://fixture/',
    url: 'https://fixture/',
    mutations: 0,
    ...state,
  };
  cdp.send = async (method, params) => {
    if (params?.expression === 'globalThis.__octoTarget')
      return { result: { objectId: 'target' } };
    if (
      `${params?.expression}${params?.functionDeclaration}`.includes(
        'const texts'
      )
    )
      return { result: { value: null } };
    if (method === 'Runtime.callFunctionOn' || params?.awaitPromise)
      return { result: { value: { ...target } } };
    return { result: { value: { ready: 'complete', content: true } } };
  };
};

test('a request cut short by the step budget names the budget, not a tiny CDP timeout', async () =>
  fixture(async ({ cdp, read }) => {
    const sends = cdp.send;
    cdp.send = async (method, params, sessionId, options) => {
      if (method !== 'Runtime.evaluate')
        return sends(method, params, sessionId);
      await new Promise(resolve => setTimeout(resolve, options.timeoutMs));
      throw new Error(`CDP timeout (${options.timeoutMs}ms) for: ${method}`);
    };
    const failure = await failureOf(cdp, read, {
      commandMs: 5000,
      steps: [
        {
          op: 'cdp',
          method: 'Runtime.evaluate',
          params: {},
          acknowledge: true,
          timeoutMs: 40,
        },
      ],
    });
    assert.match(failure.error, /step budget exhausted/);
    assert.match(failure.error, /raise timeoutMs/);
    assert.match(failure.error, /Runtime\.evaluate/);
    assert.doesNotMatch(failure.error, /CDP timeout \(\d+ms\)/);
    assert.equal(failure.outcomeUncertain, true);
  }));

test('a request that times out within the step budget keeps its CDP timeout message', async () =>
  fixture(async ({ cdp, read }) => {
    cdp.send = async (method, params, sessionId, options) => {
      throw new Error(`CDP timeout (${options.timeoutMs}ms) for: ${method}`);
    };
    const failure = await failureOf(cdp, read, {
      commandMs: 20,
      steps: [
        {
          op: 'cdp',
          method: 'Runtime.evaluate',
          params: {},
          acknowledge: true,
          timeoutMs: 5000,
        },
      ],
    });
    assert.match(failure.error, /CDP timeout \(20ms\) for: Runtime\.evaluate/);
  }));

test('a missed text wait names the text it waited for', async () =>
  fixture(async ({ cdp, read }) => {
    pageReporting(cdp, {});
    const failure = await failureOf(cdp, read, {
      steps: [{ op: 'wait', value: 'Prompt cache ready', timeoutMs: 200 }],
    });
    assert.match(failure.error, /WAIT_TIMEOUT/);
    assert.match(failure.error, /Prompt cache ready/);
    assert.match(failure.error, /200ms/);
  }));

// A selector wait whose setup reads are slow: the wait loop starts late, so measured from its own start it would
// run past the step deadline and the first read after it fails as a generic deadline error.
test('a selector wait on a slow page fails with WAIT_TIMEOUT naming the target, not the step deadline', async () =>
  fixture(async ({ cdp, read }) => {
    pageReporting(cdp, { found: false, visible: false, canOperate: false });
    const reads = cdp.send;
    cdp.send = async (...args) => {
      await new Promise(resolve => setTimeout(resolve, 40));
      return reads(...args);
    };
    const failure = await failureOf(cdp, read, {
      steps: [{ op: 'wait', selector: '#never', timeoutMs: 300 }],
    });
    assert.match(failure.error, /WAIT_TIMEOUT/);
    assert.match(failure.error, /#never/);
    assert.doesNotMatch(failure.error, /step deadline/);
  }));

test('a step deadline hit before a click finds its target still names the deadline', async () =>
  fixture(async ({ cdp, read }) => {
    pageReporting(cdp, { found: false, visible: false, canOperate: false });
    const reads = cdp.send;
    cdp.send = async (...args) => {
      await new Promise(resolve => setTimeout(resolve, 120));
      return reads(...args);
    };
    const failure = await failureOf(cdp, read, {
      steps: [
        { op: 'act', action: 'click', selector: '#never', timeoutMs: 100 },
      ],
    });
    assert.match(failure.error, /step deadline|step budget/);
  }));

/** @type {Array<[string, object, object, RegExp, RegExp]>} */
const causeCases = [
  [
    'a verify mismatch',
    { op: 'act', action: 'select', selector: '#status', value: 'In progress' },
    { operation: 'selected', state: { selected: 'To do' } },
    /VERIFY_MISMATCH.*action:"type"/s,
    /DOM_ACTION|DOM_INPUT/,
  ],
  [
    'a click with no visible effect and a missed after.text',
    {
      op: 'act',
      action: 'click',
      selector: '#go',
      after: { text: 'Route: #route-a' },
      timeoutMs: 200,
    },
    { operation: 'clicked' },
    /NO_VISIBLE_EFFECT.*after\.text.*timeoutMs/s,
    /DOM_WAIT_TEXT|DOM_SETTLE_MS/,
  ],
];

for (const [name, step, details, cause, forbidden] of causeCases)
  test(`${name} reports its cause with step fields, not env vars`, async () =>
    fixture(async ({ cdp, read }) => {
      pageReporting(cdp, details);
      const failure = await failureOf(cdp, read, { steps: [step] });
      assert.match(failure.error, cause);
      assert.doesNotMatch(failure.error, forbidden);
    }));
