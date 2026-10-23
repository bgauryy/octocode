// Dialogs, popup discovery with owned cleanup, and file upload/download bytes.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { suite, cdp, ref, event, plan, events, checkList } from './harness.mjs';

const { checks, define } = checkList();
export { checks };

define('advanced: dialog accept preserves event and resumes page', [
  'Page.handleJavaScriptDialog',
  'Runtime.evaluate',
], async () => {
  const r = await plan([
    {
      op: 'goto',
      url: suite.base + '/advanced',
      after: { selector: '#dialogResult' },
    },
    cdp('Page.enable'),
    { op: 'listen', id: 'dialog', event: 'Page.javascriptDialogOpening' },
    cdp('Runtime.evaluate', {
      expression:
        "setTimeout(()=>{document.querySelector('#dialogResult').textContent=confirm('Owned confirmation')?'Dialog accepted':'Dialog dismissed'},0);true",
      returnByValue: true,
    }),
    { op: 'waitEvent', listener: 'dialog' },
    cdp(
      'Page.handleJavaScriptDialog',
      { accept: true },
      { after: { text: 'Dialog accepted' } }
    ),
  ]);
  assert.equal(events(r, 'dialog')[0].params.type, 'confirm');
  assert.equal(events(r, 'dialog')[0].params.message, 'Owned confirmation');
});
define('advanced: popup event discovery and owned target cleanup', [
  'Target.createTarget',
  'Target.attachToTarget',
  'Target.closeTarget',
  'Target.setDiscoverTargets',
], async () => {
  const parent = ref(2, '/sessionId'),
    child = ref(7, '/sessionId');
  const r = await plan(
    [
      cdp('Target.createTarget', { url: suite.base + '/advanced' }),
      cdp('Target.attachToTarget', {
        targetId: ref(1, '/targetId'),
        flatten: true,
      }),
      cdp('Target.setDiscoverTargets', { discover: true }),
      {
        op: 'listen',
        id: 'popup',
        event: 'Target.targetCreated',
        where: { '/targetInfo/type': 'page' },
      },
      cdp(
        'Runtime.evaluate',
        {
          expression: `window.open(${JSON.stringify(suite.base + '/popup')},'_blank');true`,
          returnByValue: true,
          userGesture: true,
        },
        { session: parent }
      ),
      { op: 'waitEvent', listener: 'popup' },
      cdp('Target.attachToTarget', {
        targetId: event('popup', '/targetInfo/targetId'),
        flatten: true,
      }),
      { op: 'wait', value: 'Popup ready', session: child },
      cdp(
        'Runtime.evaluate',
        {
          expression: '({title:document.title,body:document.body.textContent})',
          returnByValue: true,
        },
        { session: child }
      ),
      cdp('Target.closeTarget', {
        targetId: event('popup', '/targetInfo/targetId'),
      }),
      cdp('Target.detachFromTarget', { sessionId: parent }),
      cdp('Target.closeTarget', { targetId: ref(1, '/targetId') }),
    ],
    ['--browser']
  );
  assert.equal(r.read(9).result.value.title, 'Owned popup');
  assert(r.read(9).result.value.body.includes('Popup ready'));
  assert.equal(r.read(10).success, true);
  assert.equal(r.read(12).success, true);
  assert.equal(
    events(r, 'popup').filter(row => row.params.targetInfo.type === 'page')
      .length,
    1
  );
});
define('advanced: owned file upload and completed download bytes', [
  'DOM.setFileInputFiles',
  'Browser.setDownloadBehavior',
], async () => {
  const upload = join(suite.work, 'owned-upload.txt');
  writeFileSync(upload, 'owned upload evidence');
  const uploaded = await plan([
    {
      op: 'goto',
      url: suite.base + '/advanced',
      after: { selector: '#upload' },
    },
    cdp('DOM.getDocument'),
    cdp('DOM.querySelector', {
      nodeId: ref(2, '/root/nodeId'),
      selector: '#upload',
    }),
    cdp(
      'DOM.setFileInputFiles',
      { nodeId: ref(3, '/nodeId'), files: [upload] },
      { after: { text: 'owned-upload.txt:21' } }
    ),
    cdp('Runtime.evaluate', {
      expression: "document.querySelector('#upload').files[0].text()",
      awaitPromise: true,
      returnByValue: true,
    }),
  ]);
  assert.equal(uploaded.read(5).result.value, 'owned upload evidence');
  const downloadPath = join(suite.work, 'downloads');
  mkdirSync(downloadPath);
  const downloaded = await plan(
    [
      cdp('Browser.setDownloadBehavior', {
        behavior: 'allow',
        downloadPath,
        eventsEnabled: true,
      }),
      { op: 'listen', id: 'download', event: 'Browser.downloadWillBegin' },
      {
        op: 'listen',
        id: 'complete',
        event: 'Browser.downloadProgress',
        where: { '/state': 'completed' },
      },
      cdp('Target.createTarget', { url: suite.base + '/advanced' }),
      cdp('Target.attachToTarget', {
        targetId: ref(4, '/targetId'),
        flatten: true,
      }),
      { op: 'wait', selector: '#download', session: ref(5, '/sessionId') },
      {
        op: 'act',
        selector: '#download',
        action: 'click',
        session: ref(5, '/sessionId'),
      },
      { op: 'waitEvent', listener: 'download' },
      { op: 'waitEvent', listener: 'complete' },
      cdp('Target.closeTarget', { targetId: ref(4, '/targetId') }),
      cdp('Browser.setDownloadBehavior', {
        behavior: 'default',
        eventsEnabled: false,
      }),
    ],
    ['--browser']
  );
  assert.equal(
    events(downloaded, 'download')[0].params.suggestedFilename,
    'owned-fixture.txt'
  );
  assert.equal(
    events(downloaded, 'complete').filter(
      row => row.params.state === 'completed'
    ).length,
    1
  );
  assert.equal(
    readFileSync(join(downloadPath, 'owned-fixture.txt'), 'utf8'),
    'owned download evidence\n'
  );
});
