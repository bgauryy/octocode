// A `$step` reference points into the raw CDP response of that step: whatever nesting Chrome
// returns (`/root/nodeId`, `/result/objectId`, `/nodeId`, `/object/objectId`, `/computedStyle/0/name`)
// is the pointer, and nothing is wrapped around it. Real Chrome; skipped without Chrome.
import test from 'node:test';
import assert from 'node:assert/strict';
import { skip, withSite } from './tool-depth-fixture.mjs';

const cdp = (method, params) => ({
  op: 'cdp',
  method,
  acknowledge: true,
  ...(params && { params }),
});
const ref = (step, pointer) => ({ $step: step, pointer });

test(
  'a $step pointer is relative to the raw CDP response of that step',
  { skip, timeout: 120000 },
  async () => {
    await withSite(async ({ connection, call }) => {
      const chains = {
        'Runtime.evaluate nests objectId under /result': [
          cdp('Runtime.evaluate', { expression: 'document.body' }),
          cdp('Runtime.callFunctionOn', {
            objectId: ref(1, '/result/objectId'),
            functionDeclaration: 'function(){return this.tagName}',
          }),
        ],
        'DOM.getDocument gives /root/nodeId, DOM.querySelector gives /nodeId': [
          cdp('DOM.getDocument'),
          cdp('DOM.querySelector', {
            nodeId: ref(1, '/root/nodeId'),
            selector: 'h1',
          }),
          cdp('DOM.describeNode', { nodeId: ref(2, '/nodeId') }),
          cdp('DOM.resolveNode', { nodeId: ref(2, '/nodeId') }),
          cdp('Runtime.callFunctionOn', {
            objectId: ref(4, '/object/objectId'),
            functionDeclaration: 'function(){return this.tagName}',
          }),
        ],
        'CSS arrays are indexed from the response root': [
          cdp('DOM.enable'),
          cdp('CSS.enable'),
          cdp('DOM.getDocument'),
          cdp('DOM.querySelector', {
            nodeId: ref(3, '/root/nodeId'),
            selector: 'h1',
          }),
          cdp('CSS.getComputedStyleForNode', { nodeId: ref(4, '/nodeId') }),
          cdp('DOM.setAttributeValue', {
            nodeId: ref(4, '/nodeId'),
            name: ref(5, '/computedStyle/0/name'),
            value: 'x',
          }),
        ],
      };
      for (const [label, steps] of Object.entries(chains)) {
        const reply = await call('run', {
          connection,
          plan: { steps },
        });
        assert.equal(reply.refused, undefined, `${label}: ${reply.refused}`);
        assert.ok(reply.ok, label);
      }
      // A pointer wrapped under an extra /result fails for a result that is not nested there, and
      // the error shows the real top-level keys and states the rule.
      const bad = await call('run', {
        connection,
        plan: {
          steps: [
            cdp('DOM.getDocument'),
            cdp('DOM.querySelector', {
              nodeId: ref(1, '/result/root/nodeId'),
              selector: 'h1',
            }),
          ],
        },
      });
      const text = bad.refused ?? JSON.stringify(bad);
      assert.match(text, /top-level keys of step 1's result: root/);
      assert.match(text, /raw CDP response/);
      assert.match(text, /\/root\/nodeId/);
      assert.doesNotMatch(text, /NOT wrapped/);
    });
  }
);
