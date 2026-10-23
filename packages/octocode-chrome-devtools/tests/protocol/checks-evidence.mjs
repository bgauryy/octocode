// PDF/screenshot streams, WebSocket payloads, storage, emulation, large plans, readiness
// failures and Runtime failure evidence.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  suite,
  cli,
  artifact,
  cdp,
  ref,
  plan,
  navigate,
  events,
  checkList,
} from './harness.mjs';

const { checks, define } = checkList();
export { checks };

define('Page PDF stream screenshot and browser version', [
  'Page.printToPDF',
  'Page.captureScreenshot',
  'Browser.getVersion',
], async () => {
  const r = await plan(
    navigate([
      cdp('Page.printToPDF', { transferMode: 'ReturnAsStream' }),
      { op: 'readStream', handle: ref(2, '/stream'), size: 4096 },
      cdp('Page.captureScreenshot', { format: 'png' }),
    ])
  );
  const chunks = readFileSync(r.data.steps[2].artifact, 'utf8')
    .trim()
    .split('\n')
    .map(line => JSON.parse(line));
  const pdf = Buffer.concat(
    chunks.map(row =>
      Buffer.from(row.data, row.base64Encoded ? 'base64' : 'utf8')
    )
  );
  assert(pdf.subarray(0, 5).equals(Buffer.from('%PDF-')));
  assert(
    Buffer.from(r.read(4).data, 'base64')
      .subarray(0, 4)
      .equals(Buffer.from([137, 80, 78, 71]))
  );
  const version = await plan([cdp('Browser.getVersion')], ['--browser']);
  assert.equal(version.read(1).product, suite.browser);
});
define('Network WebSocket event payload', ['Network.enable'], async () => {
  const r = await plan(
    navigate([
      cdp('Network.enable'),
      {
        op: 'listen',
        id: 'socket',
        event: 'Network.webSocketFrameReceived',
      },
      cdp('Runtime.evaluate', {
        expression: `window.ws=new WebSocket("ws://127.0.0.1:${suite.fixturePort}/socket");ws.onmessage=e=>document.querySelector('#out').textContent=e.data;true`,
        returnByValue: true,
      }),
      { op: 'waitEvent', listener: 'socket' },
      cdp(
        'Runtime.evaluate',
        { expression: 'ws.close();true', returnByValue: true },
        { after: { text: 'hello' } }
      ),
    ])
  );
  assert(
    events(r, 'socket').some(row => row.params.response.payloadData === 'hello')
  );
});
define('DOMStorage IndexedDB CacheStorage and cookie metadata', [
  'DOMStorage.getDOMStorageItems',
  'IndexedDB.requestDatabaseNames',
  'CacheStorage.requestCacheNames',
  'Storage.getCookies',
], async () => {
  const r = await plan(
    navigate([
      cdp('DOMStorage.enable'),
      cdp('IndexedDB.enable'),
      cdp('Runtime.evaluate', {
        expression:
          'localStorage.setItem("audit","fixture");new Promise((resolve,reject)=>{const req=indexedDB.open("audit-db",1);req.onupgradeneeded=()=>req.result.createObjectStore("items");req.onerror=reject;req.onsuccess=()=>{req.result.close();caches.open("audit-cache").then(()=>resolve(true));}})',
        awaitPromise: true,
        returnByValue: true,
      }),
      cdp('DOMStorage.getDOMStorageItems', {
        storageId: { securityOrigin: suite.base, isLocalStorage: true },
      }),
      cdp('IndexedDB.requestDatabaseNames', { securityOrigin: suite.base }),
      cdp('CacheStorage.requestCacheNames', { securityOrigin: suite.base }),
      cdp('Storage.getCookies'),
    ])
  );
  assert(
    r.read(5).entries.some(row => row[0] === 'audit' && row[1] === 'fixture')
  );
  assert(r.read(6).databaseNames.includes('audit-db'));
  assert(r.read(7).caches.some(row => row.cacheName === 'audit-cache'));
  assert(Array.isArray(r.read(8).cookies));
});
define('Emulation metrics timezone and restore', [
  'Emulation.setDeviceMetricsOverride',
  'Emulation.setTimezoneOverride',
], async () => {
  const r = await plan(
    navigate([
      cdp('Emulation.setDeviceMetricsOverride', {
        width: 800,
        height: 600,
        deviceScaleFactor: 1,
        mobile: false,
      }),
      cdp('Emulation.setTimezoneOverride', { timezoneId: 'UTC' }),
      cdp('Runtime.evaluate', {
        expression:
          '({width:innerWidth,height:innerHeight,zone:Intl.DateTimeFormat().resolvedOptions().timeZone})',
        returnByValue: true,
      }),
      cdp('Emulation.clearDeviceMetricsOverride'),
      cdp('Emulation.setTimezoneOverride', { timezoneId: '' }),
    ])
  );
  assert.deepEqual(r.read(4).result.value, {
    width: 800,
    height: 600,
    zone: 'UTC',
  });
});
define('large file-backed plans execute without environment size limits', [
  'Runtime.evaluate',
], async () => {
  const file = join(suite.work, 'large-plan.json'),
    expression = JSON.stringify('x'.repeat(4000000)) + '.length';
  writeFileSync(
    file,
    JSON.stringify({
      steps: [cdp('Runtime.evaluate', { expression, returnByValue: true })],
    })
  );
  const r = await cli([
    'run',
    '--new-tab',
    'about:blank',
    '--close-tab',
    '--plan',
    file,
  ]);
  assert.equal(r.code, 0, r.out);
  assert.equal(artifact(r.out, 'cdp-1.json').result.value, 4000000);
});
define('readiness: literal text and transparent selectors fail correctly', [
  'Runtime.evaluate',
], async () => {
  const literal = await cli([
    'run',
    '--new-tab',
    suite.base,
    '--close-tab',
    '--json',
    JSON.stringify({
      waitMs: 500,
      steps: [
        {
          op: 'act',
          role: 'button',
          name: 'Go',
          action: 'click',
          after: { text: 'Clicked|Absent' },
        },
      ],
    }),
  ]);
  assert.notEqual(
    literal.code,
    0,
    'after.text was treated as alternatives instead of literal text'
  );
  assert.equal(
    artifact(literal.out, 'browser-result.json').steps[0].status,
    'failed'
  );
  const transparent = await cli([
    'run',
    '--new-tab',
    suite.base,
    '--close-tab',
    '--json',
    JSON.stringify({
      waitMs: 500,
      steps: [
        cdp(
          'Runtime.evaluate',
          {
            expression:
              'const hidden=document.createElement("div");hidden.id="hidden";hidden.style="width:10px;height:10px;opacity:0";document.body.appendChild(hidden);true',
            returnByValue: true,
          },
          { after: { selector: '#hidden' } }
        ),
      ],
    }),
  ]);
  assert.notEqual(
    transparent.code,
    0,
    'after.selector matched a transparent control'
  );
});
define('Runtime failure preserves payload and stops later mutations', [
  'Runtime.evaluate',
], async () => {
  const r = await cli([
    'run',
    '--new-tab',
    'about:blank',
    '--close-tab',
    '--json',
    JSON.stringify({
      steps: [
        cdp('Runtime.evaluate', {
          expression: 'throw new Error("audit failure")',
          returnByValue: true,
        }),
        cdp('Runtime.evaluate', { expression: 'window.shouldNotRun=true' }),
      ],
    }),
  ]);
  assert.notEqual(r.code, 0);
  const data = artifact(r.out, 'browser-result.json');
  assert.equal(data.steps.length, 1);
  assert.equal(data.steps[0].status, 'failed');
  assert(
    artifact(
      r.out,
      'cdp-1.json'
    ).exceptionDetails.exception.description.includes('audit failure')
  );
});
