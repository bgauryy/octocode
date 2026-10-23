// Offline schema validation, DOM/CSS/AX input and layout, nested frame realms and open
// shadow discovery with ambiguous-action refusal.
import assert from 'node:assert/strict';
import { validatePlan } from '../../dist/engine/cdp-checks/browser-execute.mjs';
import {
  suite,
  cli,
  artifact,
  cdp,
  ref,
  plan,
  navigate,
  withTarget,
  checkList,
} from './harness.mjs';

const { checks, define } = checkList();
export { checks };

define('schema: every advertised command/event passes offline plan validation', [], async () => {
  for (const domain of suite.protocol.domains) {
    for (const command of domain.commands ?? []) {
      validatePlan({ steps: [cdp(domain.domain + '.' + command.name)] });
      suite.planValidatedCommands++;
    }
    for (const entry of domain.events ?? []) {
      validatePlan({
        steps: [
          {
            op: 'listen',
            id: 'event',
            event: domain.domain + '.' + entry.name,
          },
        ],
      });
      suite.planValidatedEvents++;
    }
  }
  assert(suite.planValidatedCommands > 500);
  assert(suite.planValidatedEvents > 100);
});
define('DOM CSS Accessibility input and layout', [
  'DOM.getDocument',
  'DOM.querySelector',
  'CSS.getComputedStyleForNode',
  'Accessibility.getFullAXTree',
], async () => {
  const r = await plan(
    navigate([
      cdp('DOM.enable'),
      cdp('CSS.enable'),
      cdp('DOM.getDocument'),
      cdp('DOM.querySelector', {
        nodeId: ref(4, '/root/nodeId'),
        selector: '#q',
      }),
      cdp('CSS.getComputedStyleForNode', { nodeId: ref(5, '/nodeId') }),
      cdp('Accessibility.getFullAXTree'),
      {
        op: 'act',
        role: 'textbox',
        name: 'Search',
        action: 'fill',
        value: 'audit',
      },
      {
        op: 'act',
        role: 'button',
        name: 'Go',
        action: 'click',
        after: { text: 'Clicked' },
      },
      cdp('Page.getLayoutMetrics'),
    ])
  );
  assert(
    r
      .read(6)
      .computedStyle.some(
        row => row.name === 'color' && row.value === 'rgb(1, 2, 3)'
      )
  );
  assert(r.read(7).nodes.some(row => row.name?.value === 'Search'));
  assert(r.read(10).cssLayoutViewport.clientWidth > 0);
});
// Resolves once the nested inner frame shows the given generation.
const ready = generation =>
  cdp('Runtime.evaluate', {
    expression: `new Promise(resolve=>{const poll=()=>{const text=document.querySelector('#outer')?.contentDocument?.querySelector('#inner')?.contentDocument?.querySelector('#ready')?.textContent;if(text==='Inner generation ${generation}')resolve(true);else setTimeout(poll,10)};poll()})`,
    awaitPromise: true,
    returnByValue: true,
  });
define('advanced: nested and replaced iframe realms', [
  'Page.getFrameTree',
  'Page.createIsolatedWorld',
  'Runtime.evaluate',
], async () => {
  const r = await plan([
    {
      op: 'goto',
      url: suite.base + '/advanced',
      after: { selector: '#outer' },
    },
    ready(1),
    cdp('Page.getFrameTree'),
    cdp('Page.createIsolatedWorld', {
      frameId: ref(3, '/frameTree/childFrames/0/childFrames/0/frame/id'),
      worldName: 'owned-nested-1',
    }),
    cdp('Runtime.evaluate', {
      contextId: ref(4, '/executionContextId'),
      expression: "document.querySelector('#ready').textContent",
      returnByValue: true,
    }),
    { op: 'act', selector: '#replaceOuter', action: 'click' },
    ready(2),
    {
      op: 'act',
      frame: { selector: '#outer' },
      selector: '#replaceInner',
      action: 'click',
    },
    ready(3),
    cdp('Page.getFrameTree'),
    cdp('Page.createIsolatedWorld', {
      frameId: ref(10, '/frameTree/childFrames/0/childFrames/0/frame/id'),
      worldName: 'owned-nested-3',
    }),
    cdp('Runtime.evaluate', {
      contextId: ref(11, '/executionContextId'),
      expression: "document.querySelector('#ready').textContent",
      returnByValue: true,
    }),
  ]);
  assert.equal(r.read(5).result.value, 'Inner generation 1');
  assert.equal(r.read(12).result.value, 'Inner generation 3');
  assert.notEqual(
    r.read(3).frameTree.childFrames[0].frame.id,
    r.read(10).frameTree.childFrames[0].frame.id
  );
});
define('advanced: open shadow discovery and duplicate action refusal', [
  'DOM.getDocument',
  'Runtime.evaluate',
], () =>
  withTarget(suite.base + '/advanced', async selection => {
    const r = await plan(
      [
        { op: 'wait', selector: '#shadow' },
        cdp('DOM.getDocument', { depth: -1, pierce: true }),
        {
          op: 'act',
          role: 'button',
          name: 'Shadow action',
          action: 'click',
        },
        cdp('Runtime.evaluate', {
          expression: "document.querySelector('#shadowResult').textContent",
          returnByValue: true,
        }),
      ],
      selection
    );
    assert(JSON.stringify(r.read(2).root).includes('shadowRoots'));
    assert.equal(r.read(4).result.value, 'Shadow clicked');
    const failed = await cli([
      'run',
      ...selection,
      '--json',
      JSON.stringify({
        waitMs: 6000,
        steps: [
          {
            op: 'act',
            role: 'button',
            name: 'Duplicate action',
            action: 'click',
          },
          cdp('Runtime.evaluate', {
            expression: "document.body.dataset.shouldNotRun='yes'",
          }),
        ],
      }),
    ]);
    assert.notEqual(failed.code, 0, 'Ambiguous action must fail');
    const failure = artifact(failed.out, 'browser-result.json');
    assert.equal(
      failure.steps.length,
      1,
      'Ambiguity must stop later operations'
    );
    assert.equal(
      artifact(failed.out, 'DOM_CHECK').ambiguous,
      true,
      'Saved DOM evidence must identify ambiguity'
    );
    const state = await plan(
      [
        cdp('Runtime.evaluate', {
          expression:
            '({duplicate:document.body.dataset.duplicate??null,later:document.body.dataset.shouldNotRun??null})',
          returnByValue: true,
        }),
      ],
      selection
    );
    assert.deepEqual(state.read(1).result.value, {
      duplicate: null,
      later: null,
    });
  }));
