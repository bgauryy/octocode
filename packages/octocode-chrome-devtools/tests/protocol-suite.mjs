#!/usr/bin/env node
// Isolated local fixtures exercise generic CDP plans across representative domains.
// The installed schema inventory proves dispatch reachability, not every method's semantics.
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { helpers } from '../dist/engine/cli-catalog.mjs';
import { validatePlan } from '../dist/engine/cdp-checks/browser-execute.mjs';

const args = process.argv.slice(2),
  get = flag => args[args.indexOf(flag) + 1],
  scripts = join(import.meta.dirname, '../dist/engine');
if (args.includes('--help')) {
  console.log(
    'Usage: protocol-suite.mjs [--only <name>] [--report <JSON file>] [--transport legacy|mcp|typed-cli] [--keep]\nLocal Chrome fixtures: schema dispatch, DOM/CSS/AX, browser and worker sessions, interception, debugger/profiler, heap, trace streams, PDF, sockets, storage, emulation and failure evidence. Advertised methods are inventoried separately from methods executed.'
  );
  process.exit(0);
}
if (args.includes('--serve')) {
  const server = createServer((req, res) => {
    const route = new URL(req.url, 'http://fixture');
    if (route.pathname === '/isolated-parent') {
      res.setHeader('content-type', 'text/html');
      res.end(
        `<!doctype html><title>Cross site parent</title><iframe id="isolated" src="http://localhost:${server.address().port}/isolated?generation=1"></iframe><button id="replace" onclick="document.querySelector('#isolated').outerHTML='<iframe id=isolated src=http://localhost:${server.address().port}/isolated?generation=2></iframe>'">Replace isolated</button>`
      );
      return;
    }
    if (route.pathname === '/isolated') {
      res.setHeader('content-type', 'text/html');
      res.end(
        `<!doctype html><p id="ready">Isolated generation ${route.searchParams.get('generation')}</p><button id="advance" onclick="document.querySelector('#ready').textContent='Isolated clicked'">Advance isolated</button>`
      );
      return;
    }
    if (route.pathname === '/owned-sw.js') {
      res.setHeader('content-type', 'text/javascript');
      res.setHeader('service-worker-allowed', '/sw/');
      res.end(
        `self.addEventListener('install',e=>e.waitUntil(self.skipWaiting()));self.addEventListener('activate',e=>e.waitUntil(self.clients.claim()));self.addEventListener('fetch',e=>{if(new URL(e.request.url).pathname==='/sw/response')e.respondWith(new Response(JSON.stringify({source:'service-worker',message:'owned'}),{headers:{'content-type':'application/json'}}))});`
      );
      return;
    }
    if (route.pathname === '/sw/response') {
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ source: 'network', message: 'owned' }));
      return;
    }
    if (route.pathname === '/sw/page') {
      res.setHeader('content-type', 'text/html');
      res.end(
        '<!doctype html><title>Owned worker client</title><p id="ready">Worker client ready</p>'
      );
      return;
    }
    if (route.pathname === '/stream-download') {
      res.setHeader('content-type', 'application/octet-stream');
      res.setHeader(
        'content-disposition',
        'attachment; filename="owned-stream.bin"'
      );
      let offset = 0;
      const total = 2 * 1024 * 1024;
      const timer = setInterval(() => {
        const chunk = Buffer.alloc(Math.min(32768, total - offset));
        for (let i = 0; i < chunk.length; i++) chunk[i] = (offset + i) % 251;
        res.write(chunk);
        offset += chunk.length;
        if (offset === total) {
          clearInterval(timer);
          res.end();
        }
      }, 10);
      res.on('close', () => clearInterval(timer));
      return;
    }
    if (route.pathname === '/redirect-owned') {
      res.writeHead(302, { location: '/ready-late' });
      res.end();
      return;
    }
    if (route.pathname === '/ready-late') {
      res.setHeader('content-type', 'text/html');
      res.end(`<!doctype html><h1 id="heading">Results</h1><section id="rows"></section><input id="visible" aria-label="Search owned"><input hidden aria-label="Search owned"><div id="closed"></div><script>
        window.initialRows=document.querySelectorAll('.record').length;
        setTimeout(()=>{document.querySelector('#rows').innerHTML='<p class=record>Owned record one</p><p class=record>Owned record two</p>'},350);
        const root=document.querySelector('#closed').attachShadow({mode:'closed'});root.innerHTML='<button id=closedButton>Closed action</button>';
      </script>`);
      return;
    }
    if (route.pathname === '/download') {
      res.setHeader('content-type', 'text/plain');
      res.setHeader(
        'content-disposition',
        'attachment; filename="owned-fixture.txt"'
      );
      res.end('owned download evidence\n');
      return;
    }
    if (route.pathname === '/advanced') {
      res.setHeader('content-type', 'text/html');
      res.end(`<!doctype html><title>Advanced owned fixture</title>
        <button id="replaceOuter" onclick="document.querySelector('#outer').outerHTML='<iframe id=outer title=Outer src=/outer?generation=2></iframe>'">Replace outer</button>
        <iframe id="outer" title="Outer" src="/outer?generation=1"></iframe>
        <div id="shadow"></div><output id="shadowResult">Shadow idle</output>
        <button class="duplicate" onclick="document.body.dataset.duplicate='first'">Duplicate action</button>
        <button class="duplicate" onclick="document.body.dataset.duplicate='second'">Duplicate action</button>
        <label>Owned upload <input id="upload" type="file" onchange="document.querySelector('#uploadResult').textContent=this.files[0].name+':'+this.files[0].size"></label><output id="uploadResult">Upload idle</output>
        <a id="download" href="/download">Download fixture</a>
        <output id="dialogResult">Dialog idle</output>
        <script>const shadow=document.querySelector('#shadow').attachShadow({mode:'open'});shadow.innerHTML='<button id="shadowButton">Shadow action</button>';shadow.querySelector('button').onclick=()=>document.querySelector('#shadowResult').textContent='Shadow clicked';</script>`);
      return;
    }
    if (route.pathname === '/outer') {
      const generation = route.searchParams.get('generation');
      res.setHeader('content-type', 'text/html');
      res.end(
        `<!doctype html><title>Outer ${generation}</title><p>Outer generation ${generation}</p><button id="replaceInner" onclick="document.querySelector('#inner').outerHTML='<iframe id=inner title=Inner src=/inner?generation=3></iframe>'">Replace inner</button><iframe id="inner" title="Inner" src="/inner?generation=${generation}"></iframe>`
      );
      return;
    }
    if (route.pathname === '/inner' || route.pathname === '/popup') {
      res.setHeader('content-type', 'text/html');
      res.end(
        `<!doctype html><title>Owned ${route.pathname.slice(1)}</title><p id="ready">${route.pathname === '/popup' ? 'Popup ready' : 'Inner generation ' + route.searchParams.get('generation')}</p>`
      );
      return;
    }
    if (req.url === '/worker.js') {
      res.setHeader('content-type', 'text/javascript');
      res.end('self.onmessage=e=>postMessage(e.data);');
      return;
    }
    if (req.url === '/data') {
      res.setHeader('content-type', 'application/json');
      res.end('{"message":"Data ready"}');
      return;
    }
    res.setHeader('content-type', 'text/html');
    res.end(
      '<!doctype html><title>CDP audit fixture</title><style>#q{color:rgb(1,2,3)}</style><label>Search <input id="q"></label><button onclick="document.querySelector(\'#out\').textContent=\'Clicked\'">Go</button><output id="out">Ready</output>'
    );
  });
  server.on('upgrade', (req, socket) => {
    const accept = createHash('sha1')
      .update(
        req.headers['sec-websocket-key'] +
          '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'
      )
      .digest('base64');
    socket.write(
      'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ' +
        accept +
        '\r\n\r\n'
    );
    socket.write(Buffer.concat([Buffer.from([0x81, 5]), Buffer.from('hello')]));
    socket.on('error', async () => {});
  });
  server.listen(0, '127.0.0.1', () => console.log(server.address().port));
  await new Promise(() => {});
}
const work = mkdtempSync(join(tmpdir(), 'octo-protocol-')),
  port = String(9600 + Math.floor(Math.random() * 300));
const fixture = spawn(
  process.execPath,
  [fileURLToPath(import.meta.url), '--serve'],
  { stdio: ['ignore', 'pipe', 'inherit'] }
);
const fixturePort = await new Promise((yes, no) => {
  const timer = setTimeout(
    () => no(Error('Fixture server did not start')),
    5000
  );
  fixture.stdout.once('data', data => {
    clearTimeout(timer);
    yes(String(data).trim());
  });
  fixture.once('error', no);
});
const base = 'http://127.0.0.1:' + fixturePort;
const results = [],
  tested = new Set(),
  advertised = new Set();
let protocol,
  browser,
  dispatchCommands = 0,
  dispatchEvents = 0;
const transport = args.includes('--transport') ? get('--transport') : 'legacy';
if (!['legacy', 'mcp', 'typed-cli'].includes(transport))
  throw Error('Unknown transport ' + transport);
const { connectStdio } =
  transport === 'legacy' ? {} : await import('../dist/runtime.js');
const client =
  transport === 'mcp'
    ? await connectStdio({
        command: process.execPath,
        args: [join(scripts, '../../bin/octocode-chrome-devtools.mjs')],
        cwd: work,
      })
    : null;
async function cli(argv, timeout = 45000) {
  if (transport !== 'legacy') {
    const name = argv[0],
      input = {},
      connection = { port: Number(port) };
    if (['open', 'cleanup', 'artifact'].includes(name))
      input.args = [
        ...argv.slice(1),
        ...(name === 'artifact' ? [] : ['--port', port]),
      ];
    else {
      for (let i = 1; i < argv.length; i++) {
        const key = argv[i];
        if (key === '--json') input.plan = JSON.parse(argv[++i]);
        else if (key === '--plan')
          input.plan = JSON.parse(readFileSync(argv[++i], 'utf8'));
        else if (
          ['--new-tab', '--script-timeout', '--target', '--timeout'].includes(
            key
          )
        ) {
          const value = argv[++i],
            field = key
              .slice(2)
              .replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
          connection[field] = ['scriptTimeout', 'timeout'].includes(field)
            ? Number(value)
            : value;
        } else if (['--browser', '--close-tab', '--keep-tab'].includes(key))
          connection[
            key
              .slice(2)
              .replace(/-([a-z])/g, (_, letter) => letter.toUpperCase())
          ] = true;
        else if (!key.startsWith('--') && name === 'protocol')
          input.member = key;
        else throw Error('Unmapped fixture argument ' + key);
      }
      input.connection = connection;
    }
    let message;
    if (client) {
      const response = await client.callTool(
        { name, arguments: input },
        undefined,
        { timeout }
      );
      message =
        response.structuredContent ??
        JSON.parse(response.content.find(row => row.type === 'text').text);
    } else {
      const file = join(work, 'typed-input.json');
      writeFileSync(file, JSON.stringify(input));
      const r = spawnSync(
        process.execPath,
        [
          join(scripts, '../../bin/octocode-chrome-devtools.mjs'),
          '/cli',
          name,
          '--input',
          file,
          '--json',
        ],
        { cwd: work, encoding: 'utf8', timeout }
      );
      if (r.error) throw r.error;
      message = JSON.parse((r.stdout || r.stderr).trim());
      message = message.structuredContent ?? message;
    }
    const stdout =
      message.data !== undefined
        ? JSON.stringify(message.data)
        : readFileSync(
            (
              message.logs ??
              JSON.parse(readFileSync(message.capture.file, 'utf8')).logs
            ).stdout,
            'utf8'
          );
    const stderr = message.logs?.stderr
      ? readFileSync(message.logs.stderr, 'utf8')
      : '';
    return { code: message.exitCode, out: stdout + '\n' + stderr };
  }
  const r = spawnSync(
    process.execPath,
    [
      join(scripts, 'cli.mjs'),
      ...argv,
      ...(Object.hasOwn(helpers, argv[0]) ? [] : ['--port', port]),
    ],
    { cwd: work, encoding: 'utf8', timeout }
  );
  return {
    code: r.status,
    out: (r.stdout ?? '') + (r.stderr ?? ''),
    error: r.error,
  };
}
function artifact(out, name) {
  const prefix = '[ARTIFACT] ' + name + ' ',
    line = out.split('\n').find(line => line.startsWith(prefix));
  if (!line) throw Error('Missing ' + name + ': ' + out);
  return JSON.parse(readFileSync(line.slice(prefix.length), 'utf8'));
}
const cdp = (method, params = {}, extra = {}) => ({
  op: 'cdp',
  method,
  params,
  ...extra,
});
const ref = (step, pointer) => ({ $step: step, pointer }),
  event = (id, pointer) => ({ $event: id, pointer });
async function plan(
  steps,
  selection = ['--new-tab', 'about:blank', '--close-tab'],
  options = {}
) {
  const r = await cli([
    'run',
    ...selection,
    '--script-timeout',
    '40000',
    '--json',
    JSON.stringify({ waitMs: 6000, commandMs: 6000, ...options, steps }),
  ]);
  if (r.error) throw r.error;
  const data = artifact(r.out, 'browser-result.json');
  for (const [i, row] of data.steps.entries())
    if (row.status === 'complete' && steps[i].op === 'cdp')
      tested.add(steps[i].method);
  if (r.code !== 0 || !data.ok) throw Error(r.out);
  return {
    data,
    out: r.out,
    read: i => JSON.parse(readFileSync(data.steps[i - 1].artifact, 'utf8')),
  };
}
async function check(name, needs, fn) {
  if (args.includes('--only') && !name.includes(get('--only'))) return;
  const absent = needs.filter(method => !advertised.has(method));
  if (absent.length) {
    results.push({ name, status: 'unavailable', absent });
    console.log('UNAVAILABLE ' + name + ': ' + absent.join(', '));
    return;
  }
  const start = Date.now();
  try {
    await fn();
    results.push({ name, status: 'passed', ms: Date.now() - start });
    console.log('ok ' + name + ' (' + (Date.now() - start) + 'ms)');
  } catch (error) {
    results.push({
      name,
      status: 'failed',
      ms: Date.now() - start,
      error: error.message,
    });
    console.log('FAIL ' + name + ': ' + error.message);
  }
}
function navigate(extra = []) {
  return [{ op: 'goto', url: base, after: { selector: '#q' } }, ...extra];
}
function events(result, id) {
  const file = result.data.eventCoverage.find(row => row.id === id).artifact;
  return readFileSync(file, 'utf8')
    .trim()
    .split('\n')
    .filter(Boolean)
    .map(line => JSON.parse(line));
}
try {
  const launch = await cli(['open', '--headless', '--url', 'about:blank']);
  if (launch.code !== 0) throw Error(launch.out);
  const info = JSON.parse(
    launch.out.split('\n').find(line => line.startsWith('{'))
  );
  assert.equal(
    info.reused,
    false,
    'Protocol suite requires an isolated fresh browser'
  );
  browser = info.browser;
  const capture = await cli(['protocol', '--browser']);
  assert.equal(capture.code, 0, capture.out);
  protocol = artifact(capture.out, 'cdp-protocol.json');
  for (const domain of protocol.domains)
    for (const command of domain.commands ?? [])
      advertised.add(domain.domain + '.' + command.name);
  await check(
    'schema: every advertised command/event passes generic dispatch',
    [],
    async () => {
      for (const domain of protocol.domains) {
        for (const command of domain.commands ?? []) {
          validatePlan({ steps: [cdp(domain.domain + '.' + command.name)] });
          dispatchCommands++;
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
          dispatchEvents++;
        }
      }
      assert(dispatchCommands > 500);
      assert(dispatchEvents > 100);
    }
  );
  await check(
    'DOM CSS Accessibility input and layout',
    [
      'DOM.getDocument',
      'DOM.querySelector',
      'CSS.getComputedStyleForNode',
      'Accessibility.getFullAXTree',
    ],
    async () => {
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
    }
  );
  await check(
    'advanced: nested and replaced iframe realms',
    ['Page.getFrameTree', 'Page.createIsolatedWorld', 'Runtime.evaluate'],
    async () => {
      const ready = generation =>
        cdp('Runtime.evaluate', {
          expression: `new Promise(resolve=>{const poll=()=>{const text=document.querySelector('#outer')?.contentDocument?.querySelector('#inner')?.contentDocument?.querySelector('#ready')?.textContent;if(text==='Inner generation ${generation}')resolve(true);else setTimeout(poll,10)};poll()})`,
          awaitPromise: true,
          returnByValue: true,
        });
      const r = await plan([
        { op: 'goto', url: base + '/advanced', after: { selector: '#outer' } },
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
    }
  );
  await check(
    'advanced: open shadow discovery and duplicate action refusal',
    ['DOM.getDocument', 'Runtime.evaluate'],
    async () => {
      const target = await plan(
        [cdp('Target.createTarget', { url: base + '/advanced' })],
        ['--browser']
      );
      const selection = ['--target', target.read(1).targetId];
      try {
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
      } finally {
        await plan(
          [cdp('Target.closeTarget', { targetId: target.read(1).targetId })],
          ['--browser']
        );
      }
    }
  );
  await check(
    'advanced: dialog accept preserves event and resumes page',
    ['Page.handleJavaScriptDialog', 'Runtime.evaluate'],
    async () => {
      const r = await plan([
        {
          op: 'goto',
          url: base + '/advanced',
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
    }
  );
  await check(
    'advanced: popup event discovery and owned target cleanup',
    [
      'Target.createTarget',
      'Target.attachToTarget',
      'Target.closeTarget',
      'Target.setDiscoverTargets',
    ],
    async () => {
      const parent = ref(2, '/sessionId'),
        child = ref(7, '/sessionId');
      const r = await plan(
        [
          cdp('Target.createTarget', { url: base + '/advanced' }),
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
              expression: `window.open(${JSON.stringify(base + '/popup')},'_blank');true`,
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
              expression:
                '({title:document.title,body:document.body.textContent})',
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
    }
  );
  await check(
    'advanced: owned file upload and completed download bytes',
    ['DOM.setFileInputFiles', 'Browser.setDownloadBehavior'],
    async () => {
      const upload = join(work, 'owned-upload.txt');
      writeFileSync(upload, 'owned upload evidence');
      const uploaded = await plan([
        { op: 'goto', url: base + '/advanced', after: { selector: '#upload' } },
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
      const downloadPath = join(work, 'downloads');
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
          cdp('Target.createTarget', { url: base + '/advanced' }),
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
    }
  );
  await check(
    'remaining: cross-origin isolated iframe replacement and detach lifecycle',
    ['Target.setAutoAttach', 'Target.attachToTarget', 'Runtime.evaluate'],
    async () => {
      const parent = ref(2, '/sessionId');
      const r = await plan(
        [
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
            url: base + '/isolated-parent',
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
        ],
        ['--browser']
      );
      const first = events(r, 'firstFrame').find(
        row => row.params.targetInfo.type === 'iframe'
      ).params;
      const replacement = events(r, 'replacement').find(
        row => row.params.targetInfo.type === 'iframe'
      ).params;
      assert.notEqual(
        first.targetInfo.targetId,
        replacement.targetInfo.targetId
      );
      assert.notEqual(first.sessionId, replacement.sessionId);
      assert(
        events(r, 'detachedFrame').some(
          row => row.params.sessionId === first.sessionId
        )
      );
      assert.deepEqual(r.read(15).result.value, {
        origin: `http://localhost:${fixturePort}`,
        text: 'Isolated generation 2',
      });
      assert.equal(r.read(18).success, true);
    }
  );
  await check(
    'remaining: service-worker synthetic response and explicit network bypass',
    [
      'ServiceWorker.enable',
      'Network.setBypassServiceWorker',
      'Network.getResponseBody',
    ],
    async () => {
      const workerUrl = base + '/sw/response?mode=worker',
        networkUrl = base + '/sw/response?mode=bypass';
      const r = await plan([
        {
          op: 'listen',
          id: 'workerRequests',
          event: 'Network.requestWillBeSent',
        },
        { op: 'goto', url: base + '/sw/page', after: { selector: '#ready' } },
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
      ]);
      const requests = events(r, 'workerRequests');
      for (const url of [workerUrl, networkUrl])
        assert(
          requests.some(
            row =>
              row.params.request.url === url &&
              row.params.request.method === 'GET'
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
              version.scriptURL === base + '/owned-sw.js'
          )
        )
      );
      assert.equal(r.read(17).result.value, true);
    }
  );
  await check(
    'remaining: streamed binary download progress and exact bytes',
    ['Browser.setDownloadBehavior', 'Runtime.evaluate'],
    async () => {
      const downloadPath = join(work, 'stream-downloads');
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
          cdp('Target.createTarget', { url: base }),
          cdp('Target.attachToTarget', {
            targetId: ref(4, '/targetId'),
            flatten: true,
          }),
          cdp(
            'Runtime.evaluate',
            {
              expression: `location.href=${JSON.stringify(base + '/stream-download')};true`,
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
          row =>
            row.params.state === 'inProgress' && row.params.receivedBytes > 0
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
    }
  );
  await check(
    'remaining: delayed records redirect evidence and hidden searchbox',
    ['Network.enable', 'Runtime.evaluate'],
    async () => {
      const r = await plan([
        cdp('Network.enable'),
        { op: 'listen', id: 'redirect', event: 'Network.requestWillBeSent' },
        {
          op: 'goto',
          url: base + '/redirect-owned',
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
        row => row.params.redirectResponse?.url === base + '/redirect-owned'
      );
      assert.equal(redirect.params.redirectResponse.status, 302);
      assert.equal(redirect.params.request.url, base + '/ready-late');
    }
  );
  await check(
    'remaining: multi-file upload preserves names sizes and content',
    ['DOM.setFileInputFiles'],
    async () => {
      const files = ['one.txt', 'two.txt'].map((name, index) => {
        const file = join(work, name);
        writeFileSync(file, 'owned-' + index);
        return file;
      });
      const r = await plan([
        { op: 'goto', url: base + '/advanced', after: { selector: '#upload' } },
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
    }
  );
  await check(
    'remaining: closed-shadow protocol discovery exposes exact boundary',
    ['DOM.getDocument', 'DOM.describeNode', 'DOM.querySelector'],
    async () => {
      const r = await plan([
        {
          op: 'goto',
          url: base + '/ready-late',
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
    }
  );
  await check(
    'remaining: lazy DOM replacement refuses stale node and duplicate searchboxes',
    ['DOM.getDocument', 'DOM.resolveNode', 'Runtime.evaluate'],
    async () => {
      const target = await plan(
        [cdp('Target.createTarget', { url: base + '/ready-late' })],
        ['--browser']
      );
      const selection = ['--target', target.read(1).targetId];
      try {
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
              expression:
                '[...document.querySelectorAll("input")].map(e=>e.value)',
              returnByValue: true,
            }),
          ],
          selection
        );
        assert.deepEqual(unchanged.read(1).result.value, ['', '', '']);
      } finally {
        await plan(
          [cdp('Target.closeTarget', { targetId: target.read(1).targetId })],
          ['--browser']
        );
      }
    }
  );
  for (const replacement of ['document', 'node']) {
    await check(
      replacement === 'document'
        ? 'snapshot-ref: replaced control refuses old ref before later mutation'
        : 'snapshot-ref: DOM-only replacement refuses detached old ref before later mutation',
      ['Runtime.evaluate', 'DOM.resolveNode'],
      async () => {
        const target = await plan(
          [cdp('Target.createTarget', { url: base + '/ready-late' })],
          ['--browser']
        );
        const selection = ['--target', target.read(1).targetId];
        try {
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
              row.role === 'button' &&
              row.name === 'Owned original snapshot control'
          )?.[0];
          assert(
            oldRef,
            'snapshot must expose the original observed button ref'
          );
          await plan(
            [
              ...(replacement === 'document'
                ? [
                    {
                      op: 'goto',
                      url: base + '/ready-late?replacement=full-document',
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
        } finally {
          await plan(
            [cdp('Target.closeTarget', { targetId: target.read(1).targetId })],
            ['--browser']
          );
        }
      }
    );
  }
  await check(
    'worker startup: event-derived flattened sessions',
    ['Target.setAutoAttach', 'Runtime.runIfWaitingForDebugger'],
    async () => {
      const parent = ref(2, '/sessionId'),
        child = event('worker', '/sessionId');
      const r = await plan(
        [
          cdp('Target.createTarget', { url: base }),
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
          cdp(
            'Target.detachFromTarget',
            { sessionId: child },
            { session: parent }
          ),
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
        events(r, 'log').some(
          row => row.params.args[0].value === 'worker audit'
        )
      );
      assert(r.data.steps[9].sessionId);
    }
  );
  await check(
    'Fetch interception: pause continue and verify response',
    ['Fetch.enable', 'Fetch.continueRequest'],
    async () => {
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
    }
  );
  await check(
    'Debugger Profiler Performance pause resume and CPU profile',
    [
      'Debugger.enable',
      'Debugger.resume',
      'Profiler.start',
      'Profiler.stop',
      'Performance.getMetrics',
    ],
    async () => {
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
    }
  );
  await check(
    'HeapProfiler complete chunk capture',
    ['HeapProfiler.takeHeapSnapshot', 'Runtime.getHeapUsage'],
    async () => {
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
    }
  );
  await check(
    'Tracing IO complete stream and source-pinned paging',
    ['Tracing.start', 'Tracing.end', 'IO.read'],
    async () => {
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
    }
  );
  await check(
    'Page PDF stream screenshot and browser version',
    ['Page.printToPDF', 'Page.captureScreenshot', 'Browser.getVersion'],
    async () => {
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
      assert.equal(version.read(1).product, browser);
    }
  );
  await check(
    'Network WebSocket event payload',
    ['Network.enable'],
    async () => {
      const r = await plan(
        navigate([
          cdp('Network.enable'),
          {
            op: 'listen',
            id: 'socket',
            event: 'Network.webSocketFrameReceived',
          },
          cdp('Runtime.evaluate', {
            expression: `window.ws=new WebSocket("ws://127.0.0.1:${fixturePort}/socket");ws.onmessage=e=>document.querySelector('#out').textContent=e.data;true`,
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
        events(r, 'socket').some(
          row => row.params.response.payloadData === 'hello'
        )
      );
    }
  );
  await check(
    'DOMStorage IndexedDB CacheStorage and cookie metadata',
    [
      'DOMStorage.getDOMStorageItems',
      'IndexedDB.requestDatabaseNames',
      'CacheStorage.requestCacheNames',
      'Storage.getCookies',
    ],
    async () => {
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
            storageId: { securityOrigin: base, isLocalStorage: true },
          }),
          cdp('IndexedDB.requestDatabaseNames', { securityOrigin: base }),
          cdp('CacheStorage.requestCacheNames', { securityOrigin: base }),
          cdp('Storage.getCookies'),
        ])
      );
      assert(
        r
          .read(5)
          .entries.some(row => row[0] === 'audit' && row[1] === 'fixture')
      );
      assert(r.read(6).databaseNames.includes('audit-db'));
      assert(r.read(7).caches.some(row => row.cacheName === 'audit-cache'));
      assert(Array.isArray(r.read(8).cookies));
    }
  );
  await check(
    'Emulation metrics timezone and restore',
    ['Emulation.setDeviceMetricsOverride', 'Emulation.setTimezoneOverride'],
    async () => {
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
    }
  );
  await check(
    'large file-backed plans execute without environment size limits',
    ['Runtime.evaluate'],
    async () => {
      const file = join(work, 'large-plan.json'),
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
    }
  );
  await check(
    'readiness: literal text and transparent selectors fail correctly',
    ['Runtime.evaluate'],
    async () => {
      const literal = await cli([
        'run',
        '--new-tab',
        base,
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
        base,
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
    }
  );
  await check(
    'Runtime failure preserves payload and stops later mutations',
    ['Runtime.evaluate'],
    async () => {
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
    }
  );
} catch (error) {
  results.push({
    name: 'suite setup or unexpected failure',
    status: 'failed',
    error: error.message,
  });
} finally {
  fixture.kill();
  if (!args.includes('--keep')) {
    try {
      const cleanup = await cli(['cleanup'], 15000);
      assert.equal(cleanup.code, 0, cleanup.out);
    } catch (error) {
      results.push({
        name: 'owned browser cleanup',
        status: 'failed',
        error: error.message,
      });
    }
    if (results.some(row => row.status === 'failed'))
      console.log('Retained failed-flow captures ' + work);
    else rmSync(work, { recursive: true, force: true });
  } else console.log('Kept browser port ' + port + ', captures ' + work);
  await client?.close();
}
const failed = results.filter(row => row.status === 'failed'),
  unavailable = results.filter(row => row.status === 'unavailable');
const inventory = {
  transport,
  ...(results.some(row => row.status === 'failed') || args.includes('--keep')
    ? { evidenceRoot: work }
    : {}),
  browser,
  protocolVersion: protocol?.version,
  dispatch: { commands: dispatchCommands, events: dispatchEvents },
  testedMethods: [...tested].sort(),
  domains: (protocol?.domains ?? []).map(domain => ({
    domain: domain.domain,
    commands: (domain.commands ?? []).map(row => ({
      method: domain.domain + '.' + row.name,
      executed: tested.has(domain.domain + '.' + row.name),
    })),
    events: (domain.events ?? []).map(row => domain.domain + '.' + row.name),
  })),
  results,
};
if (args.includes('--report')) {
  const file = resolve(get('--report'));
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, JSON.stringify(inventory, null, 2) + '\n');
  console.log('[ARTIFACT] protocol-coverage.json ' + file);
  console.log(
    '[NEXT] ' +
      JSON.stringify({
        continue: {
          command: process.execPath,
          args: [
            join(scripts, 'artifact-query.mjs'),
            '--file',
            file,
            '--format',
            'json',
          ],
        },
      })
  );
}
console.log(
  JSON.stringify({
    ok: failed.length === 0,
    suite: 'chrome-cdp-protocol',
    transport,
    browser,
    domains: protocol?.domains.length,
    advertisedCommands: advertised.size,
    dispatchCommands,
    dispatchEvents,
    testedMethods: tested.size,
    passed: results.length - failed.length - unavailable.length,
    failed: failed.length,
    unavailable: unavailable.length,
  })
);
process.exitCode = failed.length ? 1 : 0;
