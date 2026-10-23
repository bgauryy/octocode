// Declarative browser-execute plans: actions, frames, streams, failures and discovery.
import { readFileSync } from 'node:fs';
import {
  live,
  check,
  success,
  open,
  execute,
  ref,
  snapshot,
  artifact,
} from './harness.mjs';

export const cases = [];
const test = (name, fn) => cases.push([name, fn]);

test('executor: search acts waits extracts and observes in one invocation', () => {
  const result = success(
    execute(
      [
        { op: 'goto', url: `${live.BASE}/app-search.html` },
        {
          op: 'act',
          role: 'textbox',
          name: 'Search collection',
          action: 'fill',
          value: 'react',
        },
        {
          op: 'act',
          role: 'button',
          name: 'Search',
          action: 'click',
          after: { text: 'Result react' },
        },
        { op: 'extract', selector: '#results a' },
        {
          op: 'cdp',
          method: 'Runtime.evaluate',
          params: { expression: 'window.submissions', returnByValue: true },
        },
        { op: 'protocol', domain: 'Input', member: 'dispatchKeyEvent' },
      ],
      { observe: { network: true } }
    )
  );
  if (artifact(result.out, 'extract-4.json')[0].text !== 'Result react')
    throw new Error('Incorrect extracted result');
  if (artifact(result.out, 'cdp-5.json').result.value !== 1)
    throw new Error('Action was repeated');
  if (
    artifact(result.out, 'protocol-6.json').commands[0].name !==
    'dispatchKeyEvent'
  )
    throw new Error('Missing protocol member');
});
test('executor: API completion and trusted events', () => {
  const result = success(
    execute(
      [
        { op: 'goto', url: `${live.BASE}/async-flow.html` },
        {
          op: 'act',
          role: 'button',
          name: 'Load content',
          action: 'click',
          after: { text: 'Content loaded' },
        },
        { op: 'extract', selector: '#results', fields: ['text'] },
        {
          op: 'cdp',
          method: 'Runtime.evaluate',
          params: {
            expression:
              'window.userEvents.filter(e=>e.type==="click").every(e=>e.trusted)',
            returnByValue: true,
          },
        },
      ],
      { observe: { network: true }, traceEvents: true }
    )
  );
  if (
    !artifact(result.out, 'extract-3.json')[0].text.includes(
      'Content loaded'
    ) ||
    !artifact(result.out, 'cdp-4.json').result.value
  )
    throw new Error('Content or trusted events missing');
});
test('executor: redirected streamed content waits for task text', () => {
  const result = success(
    execute([
      {
        op: 'goto',
        url: `${live.BASE}/redirect`,
        after: { text: 'Content loaded' },
      },
      { op: 'extract', selector: 'body', fields: ['text'] },
    ])
  );
  if (
    !artifact(result.out, 'extract-2.json')[0].text.includes('Content loaded')
  )
    throw new Error('Premature extraction');
});
test('executor: failure stops later mutations and unexpected empty fails', () => {
  const result = execute([
    { op: 'goto', url: `${live.BASE}/app-search.html` },
    { op: 'extract', selector: '#missing' },
    { op: 'act', selector: '#q', action: 'fill', value: 'unsafe retry' },
  ]);
  if (result.code === 0) throw new Error('Missing elements accepted');
  const flow = artifact(result.out, 'browser-result.json');
  if (
    flow.steps.length !== 2 ||
    flow.completedSteps !== 1 ||
    flow.failure.step !== 2
  )
    throw new Error('Failure did not stop execution');
  success(
    execute([
      { op: 'goto', url: `${live.BASE}/app-search.html` },
      { op: 'extract', selector: '#missing', allowEmpty: true },
    ])
  );
});
test('executor: validates entire plan before mutation', () => {
  const result = execute([
    { op: 'goto', url: `${live.BASE}/app-search.html` },
    { op: 'act', selector: '#q', action: 'fill', value: 'must not run' },
    { op: 'invented' },
  ]);
  if (result.code === 0 || result.out.includes('[PROGRESS] BROWSER step='))
    throw new Error('Invalid plan partially executed');
});
test('executor: isolated frame input and extraction', () => {
  const frame = { url: '/cross-frame.html' };
  const result = success(
    execute([
      { op: 'goto', url: `${live.BASE}/cross-frame` },
      {
        op: 'act',
        frame,
        role: 'textbox',
        name: 'Card number',
        action: 'fill',
        value: '4242',
      },
      {
        op: 'act',
        frame,
        role: 'button',
        name: 'Pay now',
        action: 'click',
        after: { text: 'Paid in frame' },
      },
      { op: 'extract', frame, selector: '#out', fields: ['text'] },
    ])
  );
  if (artifact(result.out, 'extract-4.json')[0].text !== 'Paid in frame')
    throw new Error('Wrong frame result');
});
test('executor: same-origin frame scoped input and extraction', () => {
  const frame = { selector: 'iframe' };
  const result = success(
    execute([
      { op: 'goto', url: `${live.BASE}/actions.html` },
      {
        op: 'act',
        frame,
        role: 'textbox',
        name: 'Card number',
        action: 'fill',
        value: '4242',
      },
      { op: 'act', frame, role: 'button', name: 'Pay now', action: 'click' },
      { op: 'extract', frame, selector: '#card', fields: ['value'] },
      { op: 'wait', value: 'paid:true' },
    ])
  );
  if (artifact(result.out, 'extract-4.json')[0].value !== '4242')
    throw new Error('Wrong inline frame value');
});

test('executor: declarative trace stream and result references', () => {
  const result = success(
    execute(
      [
        { op: 'goto', url: `${live.BASE}/app-search.html` },
        { op: 'listen', id: 'trace', event: 'Tracing.tracingComplete' },
        {
          op: 'cdp',
          method: 'Tracing.start',
          params: {
            categories: 'devtools.timeline',
            transferMode: 'ReturnAsStream',
          },
        },
        { op: 'cdp', method: 'Tracing.end' },
        { op: 'waitEvent', listener: 'trace' },
        { op: 'readStream', handle: { $event: 'trace', pointer: '/stream' } },
      ],
      { commandMs: 8000 }
    )
  );
  const flow = artifact(result.out, 'browser-result.json');
  const stream = readFileSync(flow.steps[5].artifact, 'utf8')
    .trim()
    .split('\n')
    .map(JSON.parse);
  if (
    flow.eventCoverage[0].matched !== 1 ||
    !flow.steps[5].complete ||
    !stream.at(-1).eof ||
    !stream.some(chunk => chunk.data)
  )
    throw new Error('Missing complete declarative trace evidence');
  success(
    execute([
      { op: 'goto', url: `${live.BASE}/app-search.html` },
      {
        op: 'cdp',
        method: 'Runtime.evaluate',
        params: { expression: '({answer:42})' },
      },
      {
        op: 'cdp',
        method: 'Runtime.getProperties',
        params: {
          objectId: { $step: 2, pointer: '/result/objectId' },
          ownProperties: true,
        },
      },
    ])
  );
});
test('executor: stalled CDP stops with uncertain outcome', () => {
  const result = execute(
    [
      { op: 'goto', url: `${live.BASE}/app-search.html` },
      {
        op: 'cdp',
        method: 'Runtime.evaluate',
        params: { expression: 'new Promise(()=>{})', awaitPromise: true },
      },
      { op: 'act', selector: '#q', action: 'fill', value: 'must not run' },
    ],
    { commandMs: 500 }
  );
  if (result.code === 0) throw new Error('Stalled command accepted');
  const flow = artifact(result.out, 'browser-result.json');
  if (
    flow.failure.step !== 2 ||
    !flow.failure.outcomeUncertain ||
    flow.steps.length !== 2 ||
    flow.elapsedMs > 4000
  )
    throw new Error('Stall not bounded or later step ran');
});
test('executor: navigation discovers a menu before choosing its destination', () => {
  const initial = open('page-snapshot.mjs', 'smart-navigation.html').out;
  const explored = success(
    check('browser-execute.mjs', {
      BROWSER_PLAN: JSON.stringify({
        steps: [
          {
            op: 'act',
            ref: ref(initial, 'button "Explore"'),
            action: 'click',
            after: { selector: '#menu a' },
          },
          { op: 'extract', selector: '#menu a' },
        ],
      }),
    })
  ).out;
  const links = artifact(explored, 'extract-2.json'),
    chosen = links.filter(link => link.text === 'API reference');
  if (chosen.length !== 1) throw new Error('Ambiguous observed destination');
  const current = snapshot();
  const result = success(
    check('browser-execute.mjs', {
      BROWSER_PLAN: JSON.stringify({
        steps: [
          {
            op: 'act',
            ref: ref(current, 'link "' + chosen[0].text + '"'),
            action: 'click',
            after: { text: 'Done API' },
          },
          { op: 'extract', selector: 'h1', fields: ['text'] },
        ],
      }),
    })
  );
  if (artifact(result.out, 'extract-2.json')[0].text !== 'Done API')
    throw new Error('Wrong discovered destination');
});
