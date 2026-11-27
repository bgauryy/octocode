import test from 'node:test';
import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { readFileSync } from 'node:fs';
import { waitForPageReady } from '../../dist/engine/dom-actionability.mjs';
import { run } from '../../dist/engine/cdp-checks/browser-execute.mjs';
import { fixture } from './plan-fixture.mjs';

for (const url of ['about:blank', 'about:srcdoc', 'http://fixture.local/'])
  test(`interactive document readiness does not depend on URL: ${url}`, async () => {
    const cdp = {
      async send(method, params) {
        if (method !== 'Runtime.evaluate') return {};
        return {
          result: {
            value: runInNewContext(params.expression, {
              document: {
                URL: url,
                readyState: 'complete',
                querySelector: () => ({}),
                body: { innerText: 'Ready' },
              },
            }),
          },
        };
      },
    };
    assert.equal(
      await waitForPageReady(cdp, 20, { selector: '#ready', text: 'Ready' }),
      true
    );
  });

test('readiness still rejects loading documents and missing content and propagates protocol errors', async () => {
  for (const value of [
    { ready: 'loading', content: true },
    { ready: 'complete', content: false },
  ])
    assert.equal(
      await waitForPageReady({ send: async () => ({ result: { value } }) }, 20),
      false
    );
  await assert.rejects(
    waitForPageReady(
      {
        send: async () => {
          throw new Error('CDP denied');
        },
      },
      20
    ),
    /CDP denied/
  );
});

const blocked = {
  found: true,
  visible: true,
  disabled: false,
  stable: true,
  covered: true,
  coveredBy: 'div#blocker',
  operation: 'blocked-by-actionability',
  bbox: { x: 10, y: 10, width: 40, height: 20 },
};
function domReplies(cdp, sent, details, failLookup = false, lostAck = false) {
  let checks = 0;
  cdp.targetInfo = { id: 'page', type: 'page', url: 'http://fixture.local/' };
  cdp.send = async (method, params) => {
    sent.push({ method, params });
    if (method === 'Runtime.evaluate') {
      if (params.expression === 'globalThis.__octoTarget')
        return { result: { objectId: 'button' } };
      if (params.expression.includes('const matches =')) {
        if (++checks > 1 && failLookup)
          throw new Error('CDP timeout while checking target');
        return { result: { value: { ...details } } };
      }
      return {
        result: { value: { ready: 'complete', blank: false, content: true } },
      };
    }
    if (method === 'DOM.getContentQuads')
      return { quads: [[10, 10, 50, 10, 50, 30, 10, 30]] };
    if (method === 'Runtime.callFunctionOn') return { result: { value: true } };
    if (
      method === 'Input.dispatchMouseEvent' &&
      params.type === 'mouseReleased' &&
      lostAck
    )
      throw new Error('CDP timeout after dispatch');
    return {};
  };
}

for (const deadline of [false, true])
  test(`blocked click retains observed interception and sends no input (deadline=${deadline})`, async () =>
    fixture(async ({ cdp, sent, read }) => {
      domReplies(cdp, sent, blocked, deadline);
      await run(cdp, {
        waitMs: deadline ? 1000 : 300,
        steps: [{ op: 'act', action: 'click', selector: '#advance' }],
      });
      const result = read('browser-result.json');
      assert.equal(result.ok, false);
      assert.match(result.failure.error, /div#blocker/);
      assert.ok(result.steps[0].artifact);
      const evidence = JSON.parse(
        readFileSync(result.steps[0].artifact, 'utf8')
      );
      assert.equal(evidence.coveredBy, blocked.coveredBy);
      assert.equal(evidence.visible, true);
      assert.ok(!sent.some(row => row.method.startsWith('Input.')));
      if (deadline) assert.equal(result.failure.outcomeUncertain, true);
    }));

test('lost input acknowledgement stays uncertain and is never replayed', async () =>
  fixture(async ({ cdp, sent, read }) => {
    domReplies(
      cdp,
      sent,
      { ...blocked, covered: false, coveredBy: null, pending: true },
      false,
      true
    );
    await run(cdp, {
      waitMs: 1000,
      steps: [
        { op: 'act', action: 'click', selector: '#advance' },
        { op: 'cdp', method: 'Runtime.disable' },
      ],
    });
    const result = read('browser-result.json');
    assert.equal(result.ok, false);
    assert.equal(result.failure.outcomeUncertain, true);
    assert.equal(result.coverage.unvisitedSteps, 1);
    assert.equal(
      sent.filter(
        row =>
          row.method === 'Input.dispatchMouseEvent' &&
          row.params.type === 'mouseReleased'
      ).length,
      1
    );
    assert.ok(!sent.some(row => row.method === 'Runtime.disable'));
  }));
