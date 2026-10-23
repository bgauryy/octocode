// Lazy DOM replacement: stale nodes, duplicate searchboxes and stale snapshot refs refuse
// before any later mutation.
import assert from 'node:assert/strict';
import {
  suite,
  cli,
  artifact,
  cdp,
  ref,
  plan,
  withTarget,
  checkList,
} from './harness.mjs';

const { checks, define } = checkList();
export { checks };

// A stale node from before a lazy replacement fails at DOM.resolveNode, before the later mutation.
async function refuseStaleNode(selection) {
  const stale = await cli([
    'run',
    ...selection,
    '--json',
    JSON.stringify({
      waitMs: 6000,
      steps: [
        { op: 'wait', selector: '#visible' },
        cdp('DOM.getDocument'),
        cdp('DOM.querySelector', {
          nodeId: ref(2, '/root/nodeId'),
          selector: '#visible',
        }),
        cdp(
          'Runtime.evaluate',
          {
            expression:
              'setTimeout(()=>{const next=document.createElement("input");next.id="visible";next.dataset.generation="new";next.setAttribute("aria-label","Search owned");document.querySelector("#visible").replaceWith(next)},100);true',
            returnByValue: true,
          },
          { after: { selector: 'input[data-generation="new"]' } }
        ),
        cdp('DOM.resolveNode', { nodeId: ref(3, '/nodeId') }),
        cdp('Runtime.evaluate', {
          expression: 'document.body.dataset.staleMutation="ran"',
        }),
      ],
    }),
  ]);
  assert.notEqual(stale.code, 0);
  const failure = artifact(stale.out, 'browser-result.json');
  assert.equal(failure.steps.length, 5);
  assert.match(failure.failure.error, /node|document/i);
  const current = await plan(
    [
      cdp('Runtime.evaluate', {
        expression:
          '({generation:document.querySelector("#visible").dataset.generation,later:document.body.dataset.staleMutation??null})',
        returnByValue: true,
      }),
      cdp('Runtime.evaluate', {
        expression:
          '(()=>{const next=document.createElement("input");next.id="second";next.setAttribute("aria-label","Search owned");document.body.append(next);return true})()',
        returnByValue: true,
      }),
    ],
    selection
  );
  assert.deepEqual(current.read(1).result.value, {
    generation: 'new',
    later: null,
  });
}
// Two inputs share the accessible name: a fill must refuse instead of choosing one.
async function refuseDuplicateSearchbox(selection) {
  const duplicate = await cli([
    'run',
    ...selection,
    '--json',
    JSON.stringify({
      waitMs: 6000,
      steps: [
        {
          op: 'act',
          role: 'textbox',
          name: 'Search owned',
          action: 'fill',
          value: 'must not fill',
        },
      ],
    }),
  ]);
  assert.notEqual(duplicate.code, 0);
  assert.equal(artifact(duplicate.out, 'DOM_CHECK').ambiguous, true);
  const unchanged = await plan(
    [
      cdp('Runtime.evaluate', {
        expression: '[...document.querySelectorAll("input")].map(e=>e.value)',
        returnByValue: true,
      }),
    ],
    selection
  );
  assert.deepEqual(unchanged.read(1).result.value, ['', '', '']);
}
define('remaining: lazy DOM replacement refuses stale node and duplicate searchboxes', [
  'DOM.getDocument',
  'DOM.resolveNode',
  'Runtime.evaluate',
], () =>
  withTarget(suite.base + '/ready-late', async selection => {
    await refuseStaleNode(selection);
    await refuseDuplicateSearchbox(selection);
  }));

// Adds the original control and returns its snapshot ref.
async function snapshotOriginalRef(selection) {
  await plan(
    [
      { op: 'wait', selector: '#heading' },
      cdp('Runtime.evaluate', {
        expression:
          '(()=>{globalThis.ownedSnapshotClicks=0;const button=document.createElement("button");button.id="snapshotOriginal";button.textContent="Owned original snapshot control";button.onclick=()=>globalThis.ownedSnapshotClicks++;document.body.append(button);return true})()',
        returnByValue: true,
      }),
    ],
    selection
  );
  const snapshot = await cli(['snapshot', ...selection]);
  assert.equal(snapshot.code, 0, snapshot.out);
  const saved = artifact(snapshot.out, 'PAGE_SNAPSHOT');
  const oldRef = Object.entries(saved.refs).find(
    ([, row]) =>
      row.role === 'button' && row.name === 'Owned original snapshot control'
  )?.[0];
  assert(oldRef, 'snapshot must expose the original observed button ref');
  return oldRef;
}
// Replaces the control by a full-document navigation or a DOM-only swap.
async function replaceControl(selection, replacement) {
  await plan(
    [
      ...(replacement === 'document'
        ? [
            {
              op: 'goto',
              url: suite.base + '/ready-late?replacement=full-document',
              after: { selector: '#heading' },
            },
          ]
        : []),
      cdp('Runtime.evaluate', {
        expression: `(()=>{${replacement === 'document' ? 'globalThis.ownedSnapshotClicks=0;' : ''}const next=document.createElement("button");next.id="snapshotReplacement";next.textContent="Owned replacement snapshot control";next.onclick=()=>globalThis.ownedSnapshotClicks++;${replacement === 'document' ? 'document.body.append(next)' : 'document.querySelector("#snapshotOriginal").replaceWith(next)'};return true})()`,
        returnByValue: true,
      }),
    ],
    selection
  );
}
// The old ref must fail without clicking the replacement or running the later step.
async function refuseOldRef(selection, oldRef) {
  const refused = await cli([
    'run',
    ...selection,
    '--json',
    JSON.stringify({
      waitMs: 1000,
      commandMs: 12000,
      steps: [
        { op: 'act', ref: oldRef, action: 'click' },
        cdp('Runtime.evaluate', {
          expression: 'globalThis.ownedSnapshotLaterMutation="ran"',
        }),
      ],
    }),
  ]);
  assert.notEqual(refused.code, 0);
  const failed = artifact(refused.out, 'browser-result.json');
  assert.equal(failed.steps.length, 1);
  assert.equal(failed.steps[0].status, 'failed');
  const check = artifact(refused.out, 'DOM_CHECK');
  assert.equal(check.found, false);
  assert.equal(
    check.error,
    'Stale ref recovery found 0 role/name matches; refresh the snapshot'
  );
  const unchanged = await plan(
    [
      cdp('Runtime.evaluate', {
        expression:
          '({clicks:globalThis.ownedSnapshotClicks,later:globalThis.ownedSnapshotLaterMutation??null,replacement:document.querySelector("#snapshotReplacement").textContent})',
        returnByValue: true,
      }),
    ],
    selection
  );
  assert.deepEqual(unchanged.read(1).result.value, {
    clicks: 0,
    later: null,
    replacement: 'Owned replacement snapshot control',
  });
}
for (const replacement of ['document', 'node']) {
  define(
    replacement === 'document'
      ? 'snapshot-ref: replaced control refuses old ref before later mutation'
      : 'snapshot-ref: DOM-only replacement refuses detached old ref before later mutation',
    ['Runtime.evaluate', 'DOM.resolveNode'],
    () =>
      withTarget(suite.base + '/ready-late', async selection => {
        const oldRef = await snapshotOriginalRef(selection);
        await replaceControl(selection, replacement);
        await refuseOldRef(selection, oldRef);
      })
  );
}
