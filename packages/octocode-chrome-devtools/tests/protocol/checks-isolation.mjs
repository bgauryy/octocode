// Cross-origin isolated iframe lifecycle and service-worker responses vs network bypass.
import assert from 'node:assert/strict';
import { suite, cdp, ref, event, plan, events, checkList } from './harness.mjs';

const { checks, define } = checkList();
export { checks };

// Attaches a blank target, opens the cross-site parent, clicks inside the isolated frame,
// replaces it and reads the replacement; step 15 is the read, 18 closes the target.
const isolatedFrameSteps = parent => [
  cdp('Target.createTarget', { url: 'about:blank' }),
  cdp('Target.attachToTarget', {
    targetId: ref(1, '/targetId'),
    flatten: true,
  }),
  {
    op: 'listen',
    id: 'firstFrame',
    event: 'Target.attachedToTarget',
    where: { '/targetInfo/type': 'iframe' },
    session: parent,
  },
  cdp(
    'Target.setAutoAttach',
    { autoAttach: true, waitForDebuggerOnStart: false, flatten: true },
    { session: parent }
  ),
  {
    op: 'goto',
    url: suite.base + '/isolated-parent',
    session: parent,
    after: { selector: '#isolated' },
  },
  { op: 'waitEvent', listener: 'firstFrame' },
  {
    op: 'wait',
    value: 'Isolated generation 1',
    session: event('firstFrame', '/sessionId'),
  },
  {
    op: 'act',
    selector: '#advance',
    action: 'click',
    after: { text: 'Isolated clicked' },
    session: event('firstFrame', '/sessionId'),
  },
  {
    op: 'listen',
    id: 'replacement',
    event: 'Target.attachedToTarget',
    where: { '/targetInfo/type': 'iframe' },
    session: parent,
  },
  {
    op: 'listen',
    id: 'detachedFrame',
    event: 'Target.detachedFromTarget',
    session: parent,
  },
  { op: 'act', selector: '#replace', action: 'click', session: parent },
  { op: 'waitEvent', listener: 'detachedFrame' },
  { op: 'waitEvent', listener: 'replacement' },
  {
    op: 'wait',
    value: 'Isolated generation 2',
    session: event('replacement', '/sessionId'),
  },
  cdp(
    'Runtime.evaluate',
    {
      expression:
        '({origin:location.origin,text:document.querySelector("#ready").textContent})',
      returnByValue: true,
    },
    { session: event('replacement', '/sessionId') }
  ),
  cdp(
    'Target.setAutoAttach',
    { autoAttach: false, waitForDebuggerOnStart: false, flatten: true },
    { session: parent }
  ),
  cdp('Target.detachFromTarget', { sessionId: parent }),
  cdp('Target.closeTarget', { targetId: ref(1, '/targetId') }),
];

// Registers the owned worker, fetches through it, then bypasses it for the network fetch
// and unregisters; steps 8/13 return the bodies, 10/15 read them back, 17 unregisters.
const serviceWorkerSteps = (workerUrl, networkUrl) => [
  {
    op: 'listen',
    id: 'workerRequests',
    event: 'Network.requestWillBeSent',
  },
  { op: 'goto', url: suite.base + '/sw/page', after: { selector: '#ready' } },
  {
    op: 'listen',
    id: 'workerLife',
    event: 'ServiceWorker.workerVersionUpdated',
  },
  cdp('ServiceWorker.enable'),
  cdp('Network.enable'),
  cdp('Runtime.evaluate', {
    expression:
      "navigator.serviceWorker.register('/owned-sw.js',{scope:'/sw/'}).then(()=>navigator.serviceWorker.ready).then(()=>new Promise(resolve=>{if(navigator.serviceWorker.controller)resolve(true);else navigator.serviceWorker.addEventListener('controllerchange',()=>resolve(true),{once:true})}))",
    awaitPromise: true,
    returnByValue: true,
  }),
  {
    op: 'listen',
    id: 'workerResponse',
    event: 'Network.responseReceived',
    where: { '/response/url': workerUrl },
  },
  cdp('Runtime.evaluate', {
    expression: `fetch(${JSON.stringify(workerUrl)}).then(r=>r.json())`,
    awaitPromise: true,
    returnByValue: true,
  }),
  { op: 'waitEvent', listener: 'workerResponse' },
  cdp('Network.getResponseBody', {
    requestId: event('workerResponse', '/requestId'),
  }),
  cdp('Network.setBypassServiceWorker', { bypass: true }),
  {
    op: 'listen',
    id: 'networkResponse',
    event: 'Network.responseReceived',
    where: { '/response/url': networkUrl },
  },
  cdp('Runtime.evaluate', {
    expression: `fetch(${JSON.stringify(networkUrl)}).then(r=>r.json())`,
    awaitPromise: true,
    returnByValue: true,
  }),
  { op: 'waitEvent', listener: 'networkResponse' },
  cdp('Network.getResponseBody', {
    requestId: event('networkResponse', '/requestId'),
  }),
  cdp('Network.setBypassServiceWorker', { bypass: false }),
  cdp('Runtime.evaluate', {
    expression:
      "navigator.serviceWorker.getRegistration('/sw/').then(r=>r.unregister())",
    awaitPromise: true,
    returnByValue: true,
  }),
];

define('remaining: cross-origin isolated iframe replacement and detach lifecycle', [
  'Target.setAutoAttach',
  'Target.attachToTarget',
  'Runtime.evaluate',
], async () => {
  const parent = ref(2, '/sessionId');
  const r = await plan(isolatedFrameSteps(parent), ['--browser']);
  const first = events(r, 'firstFrame').find(
    row => row.params.targetInfo.type === 'iframe'
  ).params;
  const replacement = events(r, 'replacement').find(
    row => row.params.targetInfo.type === 'iframe'
  ).params;
  assert.notEqual(first.targetInfo.targetId, replacement.targetInfo.targetId);
  assert.notEqual(first.sessionId, replacement.sessionId);
  assert(
    events(r, 'detachedFrame').some(
      row => row.params.sessionId === first.sessionId
    )
  );
  assert.deepEqual(r.read(15).result.value, {
    origin: `http://localhost:${suite.fixturePort}`,
    text: 'Isolated generation 2',
  });
  assert.equal(r.read(18).success, true);
});
define('remaining: service-worker synthetic response and explicit network bypass', [
  'ServiceWorker.enable',
  'Network.setBypassServiceWorker',
  'Network.getResponseBody',
], async () => {
  const workerUrl = suite.base + '/sw/response?mode=worker',
    networkUrl = suite.base + '/sw/response?mode=bypass';
  const r = await plan(serviceWorkerSteps(workerUrl, networkUrl));
  const requests = events(r, 'workerRequests');
  for (const url of [workerUrl, networkUrl])
    assert(
      requests.some(
        row =>
          row.params.request.url === url && row.params.request.method === 'GET'
      )
    );
  assert.deepEqual(r.read(8).result.value, {
    source: 'service-worker',
    message: 'owned',
  });
  assert.deepEqual(r.read(13).result.value, {
    source: 'network',
    message: 'owned',
  });
  assert.equal(
    events(r, 'workerResponse').find(
      row => row.params.response.url === workerUrl
    ).params.response.fromServiceWorker,
    true
  );
  assert.equal(
    events(r, 'networkResponse').find(
      row => row.params.response.url === networkUrl
    ).params.response.fromServiceWorker,
    false
  );
  assert.equal(JSON.parse(r.read(10).body).source, 'service-worker');
  assert.equal(JSON.parse(r.read(15).body).source, 'network');
  assert(
    events(r, 'workerLife').some(row =>
      row.params.versions.some(
        version =>
          version.status === 'activated' &&
          version.scriptURL === suite.base + '/owned-sw.js'
      )
    )
  );
  assert.equal(r.read(17).result.value, true);
});
