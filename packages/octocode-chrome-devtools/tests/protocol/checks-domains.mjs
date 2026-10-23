// Worker startup sessions, Fetch interception, Debugger/Profiler, heap and trace streams.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  suite,
  cli,
  cdp,
  ref,
  event,
  plan,
  navigate,
  events,
  checkList,
} from './harness.mjs';

const { checks, define } = checkList();
export { checks };

define('worker startup: event-derived flattened sessions', [
  'Target.setAutoAttach',
  'Runtime.runIfWaitingForDebugger',
], async () => {
  const parent = ref(2, '/sessionId'),
    child = event('worker', '/sessionId');
  const r = await plan(
    [
      cdp('Target.createTarget', { url: suite.base }),
      cdp('Target.attachToTarget', {
        targetId: ref(1, '/targetId'),
        flatten: true,
      }),
      {
        op: 'listen',
        id: 'worker',
        event: 'Target.attachedToTarget',
        where: { '/targetInfo/type': 'worker' },
        session: parent,
      },
      cdp(
        'Target.setAutoAttach',
        { autoAttach: true, waitForDebuggerOnStart: true, flatten: true },
        { session: parent }
      ),
      cdp(
        'Runtime.evaluate',
        {
          expression: 'window.worker=new Worker("/worker.js");true',
          returnByValue: true,
        },
        { session: parent }
      ),
      { op: 'waitEvent', listener: 'worker' },
      {
        op: 'listen',
        id: 'log',
        event: 'Runtime.consoleAPICalled',
        session: child,
      },
      cdp('Runtime.enable', {}, { session: child }),
      cdp('Runtime.runIfWaitingForDebugger', {}, { session: child }),
      cdp(
        'Runtime.evaluate',
        {
          expression: 'console.log("worker audit");typeof self.postMessage',
          returnByValue: true,
        },
        { session: child }
      ),
      { op: 'waitEvent', listener: 'log' },
      cdp('Target.detachFromTarget', { sessionId: child }, { session: parent }),
      cdp(
        'Target.setAutoAttach',
        { autoAttach: false, waitForDebuggerOnStart: false, flatten: true },
        { session: parent }
      ),
      cdp('Target.detachFromTarget', { sessionId: parent }),
      cdp('Target.closeTarget', { targetId: ref(1, '/targetId') }),
    ],
    ['--browser']
  );
  assert.equal(r.read(10).result.value, 'function');
  assert.equal(events(r, 'worker')[0].params.waitingForDebugger, true);
  assert(
    events(r, 'log').some(row => row.params.args[0].value === 'worker audit')
  );
  assert(r.data.steps[9].sessionId);
});
define('Fetch interception: pause continue and verify response', [
  'Fetch.enable',
  'Fetch.continueRequest',
], async () => {
  const r = await plan(
    navigate([
      { op: 'listen', id: 'request', event: 'Fetch.requestPaused' },
      cdp('Fetch.enable', { patterns: [{ urlPattern: '*/data' }] }),
      cdp('Runtime.evaluate', {
        expression:
          'fetch("/data").then(r=>r.json()).then(v=>document.querySelector("#out").textContent=v.message);true',
        returnByValue: true,
      }),
      { op: 'waitEvent', listener: 'request' },
      cdp(
        'Fetch.continueRequest',
        { requestId: event('request', '/requestId') },
        { after: { text: 'Data ready' } }
      ),
      cdp('Fetch.disable'),
    ])
  );
  assert(events(r, 'request')[0].params.request.url.endsWith('/data'));
});
define('Debugger Profiler Performance pause resume and CPU profile', [
  'Debugger.enable',
  'Debugger.resume',
  'Profiler.start',
  'Profiler.stop',
  'Performance.getMetrics',
], async () => {
  const r = await plan(
    navigate([
      cdp('Debugger.enable'),
      { op: 'listen', id: 'pause', event: 'Debugger.paused' },
      cdp('Runtime.evaluate', {
        expression:
          'setTimeout(()=>{debugger;document.querySelector("#out").textContent="Resumed"},0);true',
        returnByValue: true,
      }),
      { op: 'waitEvent', listener: 'pause' },
      cdp('Debugger.resume', {}, { after: { text: 'Resumed' } }),
      cdp('Profiler.enable'),
      cdp('Profiler.start'),
      cdp('Runtime.evaluate', {
        expression:
          '(()=>{let n=0;for(let i=0;i<100000;i++)n+=Math.sqrt(i);return n})()',
        returnByValue: true,
      }),
      cdp('Profiler.stop'),
      cdp('Performance.enable'),
      cdp('Performance.getMetrics'),
      cdp('Debugger.disable'),
    ])
  );
  assert(r.read(10).profile.nodes.length > 0);
  assert(r.read(12).metrics.some(row => row.name === 'JSHeapUsedSize'));
  assert(events(r, 'pause')[0].params.callFrames.length > 0);
});
define('HeapProfiler complete chunk capture', [
  'HeapProfiler.takeHeapSnapshot',
  'Runtime.getHeapUsage',
], async () => {
  const r = await plan(
    navigate([
      cdp('HeapProfiler.enable'),
      {
        op: 'listen',
        id: 'heap',
        event: 'HeapProfiler.addHeapSnapshotChunk',
      },
      cdp(
        'HeapProfiler.takeHeapSnapshot',
        { reportProgress: false },
        { timeoutMs: 20000 }
      ),
      { op: 'waitEvent', listener: 'heap' },
      cdp('Runtime.getHeapUsage'),
    ]),
    undefined,
    { commandMs: 20000 }
  );
  const heap = JSON.parse(
    events(r, 'heap')
      .map(row => row.params.chunk)
      .join('')
  );
  assert(heap.snapshot.node_count > 0);
  assert(r.read(6).usedSize > 0);
});
define('Tracing IO complete stream and source-pinned paging', [
  'Tracing.start',
  'Tracing.end',
  'IO.read',
], async () => {
  const r = await plan([
    { op: 'listen', id: 'trace', event: 'Tracing.tracingComplete' },
    cdp('Tracing.start', {
      transferMode: 'ReturnAsStream',
      categories: 'devtools.timeline',
    }),
    cdp('Runtime.evaluate', { expression: '1+1', returnByValue: true }),
    cdp('Tracing.end'),
    { op: 'waitEvent', listener: 'trace' },
    { op: 'readStream', handle: event('trace', '/stream'), size: 4096 },
  ]);
  const stream = r.data.steps[5];
  assert.equal(stream.complete, true);
  const chunks = readFileSync(stream.artifact, 'utf8')
    .trim()
    .split('\n')
    .map(line => JSON.parse(line));
  assert(chunks.at(-1).eof);
  const bytes = Buffer.concat(
    chunks.map(row =>
      Buffer.from(row.data, row.base64Encoded ? 'base64' : 'utf8')
    )
  );
  assert(Array.isArray(JSON.parse(bytes).traceEvents));
  const query = await cli([
    'artifact',
    '--file',
    stream.artifact,
    '--format',
    'text',
    '--length',
    '4000',
  ]);
  assert.equal(query.code, 0, query.out);
  assert(Buffer.byteLength(query.out) < 24000);
  assert(
    JSON.parse(query.out.split('\n')[0]).next,
    'Large stream should have a continuation'
  );
});
