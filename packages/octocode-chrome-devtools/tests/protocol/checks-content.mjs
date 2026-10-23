// Streamed downloads, delayed records after redirects, multi-file upload and closed shadow.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { suite, cdp, ref, plan, events, checkList } from './harness.mjs';

const { checks, define } = checkList();
export { checks };

define('remaining: streamed binary download progress and exact bytes', [
  'Browser.setDownloadBehavior',
  'Runtime.evaluate',
], async () => {
  const downloadPath = join(suite.work, 'stream-downloads');
  mkdirSync(downloadPath);
  const r = await plan(
    [
      cdp('Browser.setDownloadBehavior', {
        behavior: 'allow',
        downloadPath,
        eventsEnabled: true,
      }),
      {
        op: 'listen',
        id: 'streamBegin',
        event: 'Browser.downloadWillBegin',
      },
      {
        op: 'listen',
        id: 'streamProgress',
        event: 'Browser.downloadProgress',
        where: { '/state': 'completed' },
      },
      cdp('Target.createTarget', { url: suite.base }),
      cdp('Target.attachToTarget', {
        targetId: ref(4, '/targetId'),
        flatten: true,
      }),
      cdp(
        'Runtime.evaluate',
        {
          expression: `location.href=${JSON.stringify(suite.base + '/stream-download')};true`,
          returnByValue: true,
        },
        { session: ref(5, '/sessionId') }
      ),
      { op: 'waitEvent', listener: 'streamBegin' },
      { op: 'waitEvent', listener: 'streamProgress' },
      cdp('Target.closeTarget', { targetId: ref(4, '/targetId') }),
      cdp('Browser.setDownloadBehavior', {
        behavior: 'default',
        eventsEnabled: false,
      }),
    ],
    ['--browser']
  );
  const progress = events(r, 'streamProgress'),
    complete = progress.filter(row => row.params.state === 'completed');
  assert.equal(complete.length, 1);
  assert.equal(complete[0].params.receivedBytes, 2 * 1024 * 1024);
  assert(
    progress.some(
      row => row.params.state === 'inProgress' && row.params.receivedBytes > 0
    )
  );
  const expected = Buffer.alloc(2 * 1024 * 1024);
  for (let i = 0; i < expected.length; i++) expected[i] = i % 251;
  const actual = readFileSync(join(downloadPath, 'owned-stream.bin'));
  assert.equal(actual.length, expected.length);
  assert(actual.equals(expected));
  assert.equal(
    events(r, 'streamBegin')[0].params.suggestedFilename,
    'owned-stream.bin'
  );
});
define('remaining: delayed records redirect evidence and hidden searchbox', [
  'Network.enable',
  'Runtime.evaluate',
], async () => {
  const r = await plan([
    cdp('Network.enable'),
    { op: 'listen', id: 'redirect', event: 'Network.requestWillBeSent' },
    {
      op: 'goto',
      url: suite.base + '/redirect-owned',
      after: { selector: '.record' },
    },
    { op: 'extract', selector: '.record', fields: ['text'] },
    cdp('Runtime.evaluate', {
      expression:
        '({initialRows,heading:!!document.querySelector("#heading"),rows:document.querySelectorAll(".record").length})',
      returnByValue: true,
    }),
    {
      op: 'act',
      role: 'textbox',
      name: 'Search owned',
      action: 'fill',
      value: 'visible only',
    },
    cdp('Runtime.evaluate', {
      expression:
        '({visible:document.querySelector("#visible").value,hidden:document.querySelector("input[hidden]").value})',
      returnByValue: true,
    }),
  ]);
  assert.deepEqual(
    r.read(4).map(row => row.text),
    ['Owned record one', 'Owned record two']
  );
  assert.deepEqual(r.read(5).result.value, {
    initialRows: 0,
    heading: true,
    rows: 2,
  });
  assert.deepEqual(r.read(7).result.value, {
    visible: 'visible only',
    hidden: '',
  });
  const redirect = events(r, 'redirect').find(
    row => row.params.redirectResponse?.url === suite.base + '/redirect-owned'
  );
  assert.equal(redirect.params.redirectResponse.status, 302);
  assert.equal(redirect.params.request.url, suite.base + '/ready-late');
});
define('remaining: multi-file upload preserves names sizes and content', [
  'DOM.setFileInputFiles',
], async () => {
  const files = ['one.txt', 'two.txt'].map((name, index) => {
    const file = join(suite.work, name);
    writeFileSync(file, 'owned-' + index);
    return file;
  });
  const r = await plan([
    {
      op: 'goto',
      url: suite.base + '/advanced',
      after: { selector: '#upload' },
    },
    cdp('Runtime.evaluate', {
      expression: 'document.querySelector("#upload").multiple=true;true',
      returnByValue: true,
    }),
    cdp('DOM.getDocument'),
    cdp('DOM.querySelector', {
      nodeId: ref(3, '/root/nodeId'),
      selector: '#upload',
    }),
    cdp('DOM.setFileInputFiles', { nodeId: ref(4, '/nodeId'), files }),
    cdp('Runtime.evaluate', {
      expression:
        'Promise.all([...document.querySelector("#upload").files].map(async f=>({name:f.name,size:f.size,text:await f.text()})))',
      awaitPromise: true,
      returnByValue: true,
    }),
  ]);
  assert.deepEqual(r.read(6).result.value, [
    { name: 'one.txt', size: 7, text: 'owned-0' },
    { name: 'two.txt', size: 7, text: 'owned-1' },
  ]);
});
define('remaining: closed-shadow protocol discovery exposes exact boundary', [
  'DOM.getDocument',
  'DOM.describeNode',
  'DOM.querySelector',
], async () => {
  const r = await plan([
    {
      op: 'goto',
      url: suite.base + '/ready-late',
      after: { selector: '#closed' },
    },
    cdp('DOM.getDocument', { depth: -1, pierce: true }),
    cdp('DOM.querySelector', {
      nodeId: ref(2, '/root/nodeId'),
      selector: '#closed',
    }),
    cdp('DOM.describeNode', {
      nodeId: ref(3, '/nodeId'),
      depth: -1,
      pierce: true,
    }),
    cdp('DOM.querySelector', {
      nodeId: ref(4, '/node/shadowRoots/0/nodeId'),
      selector: '#closedButton',
    }),
    cdp('DOM.describeNode', { nodeId: ref(5, '/nodeId') }),
    cdp('Runtime.evaluate', {
      expression: 'document.querySelector("#closed").shadowRoot===null',
      returnByValue: true,
    }),
  ]);
  assert.equal(r.read(4).node.shadowRoots[0].shadowRootType, 'closed');
  assert.equal(r.read(6).node.nodeName, 'BUTTON');
  assert.equal(r.read(7).result.value, true);
});
