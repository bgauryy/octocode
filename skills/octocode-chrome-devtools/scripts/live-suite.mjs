#!/usr/bin/env node
// Live regression suite: local fixtures + isolated headless Chrome + the real check scripts.
// Fixtures: scripts/tests/fixtures/actions.html (+ scripts/tests/fixtures/frame.html iframe),
// scripts/tests/fixtures/list.html, scripts/tests/fixtures/perf.html.
// Needs Chrome; no network. Usage: node scripts/live-suite.mjs [--keep] [--only <name-substring>]
import { spawnSync, spawn } from 'child_process';
import { createServer } from 'http';
import { readFileSync, existsSync, writeFileSync, mkdtempSync, rmSync } from 'fs';
import { dirname, join } from 'path';
import { tmpdir } from 'os';
import { fileURLToPath } from 'url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const FIXTURES = join(ROOT, 'scripts', 'tests', 'fixtures');
const args = process.argv.slice(2);
if (args.includes('-h') || args.includes('--help')) {
  console.log('Usage: live-suite.mjs [--keep] [--only <substring>]\n\nRuns snapshot, action, wait, upload, drag, iframe, screenshot and perf checks against local fixtures in an isolated headless Chrome.');
  process.exit(0);
}
const SERVE = args.includes('--serve-fixtures'); // internal: child process that serves the fixtures
const KEEP = args.includes('--keep');
const ONLY = args.includes('--only') ? args[args.indexOf('--only') + 1] : '';
const CDP_PORT = String(9400 + Math.floor(Math.random() * 400));
// Checks stage helpers and write artifacts under <cwd>/.octocode; keep that out of the skill folder.
const WORK = SERVE ? null : mkdtempSync(join(tmpdir(), 'octo-live-'));

const server = createServer((req, res) => {
  const url = new URL(req.url, 'http://fixture');
  if (url.pathname === '/stream') {
    res.setHeader('content-type', 'text/plain'); res.flushHeaders(); res.write('Content ');
    setTimeout(() => res.end('loaded'), 700); return;
  }
  if (url.pathname === '/redirect') { res.writeHead(302, { location: '/stream' }); res.end(); return; }
  if (url.pathname === '/pending') { res.setHeader('content-type', 'text/plain'); res.flushHeaders(); setTimeout(() => res.end('late'), 2500); return; }
  if (url.pathname === '/network-flow') { res.setHeader('content-type', 'text/html'); res.end('<title>Network flow</title><script>fetch("/redirect");fetch("/pending");</script>'); return; }
  if (url.pathname === '/frame-network-flow') { res.setHeader('content-type', 'text/html'); res.end(`<title>Frame network flow</title><iframe src="http://localhost:${server.address().port}/network-child"></iframe>`); return; }
  if (url.pathname === '/network-child') { res.setHeader('content-type', 'text/html'); res.end('<title>Network child</title><script>setTimeout(()=>fetch("/stream"),300)</script>'); return; }
  if (url.pathname === '/cross-frame') { res.setHeader('content-type', 'text/html'); res.end(`<title>Cross-origin frame</title><h1>Cross-origin frame</h1><iframe title="Payment" src="http://localhost:${server.address().port}/cross-frame.html" width="400" height="120"></iframe>`); return; }
  if (url.pathname === '/done') {
    res.setHeader('content-type', 'text/html');
    res.end(`<title>Done</title><h1>Done ${String(url.searchParams.get('q')).replace(/[<&]/g, '')}</h1>`);
    return;
  }
  const file = join(FIXTURES, url.pathname.replace(/^\/+/, '') || 'actions.html');
  if (!file.startsWith(FIXTURES) || !existsSync(file)) { res.statusCode = 404; res.end('not found'); return; }
  res.setHeader('content-type', 'text/html');
  res.end(readFileSync(file));
});
if (SERVE) {
  server.listen(0, '127.0.0.1', () => console.log(server.address().port));
  await new Promise(() => {});
}
// The checks run through spawnSync, which blocks this event loop, so the server lives in a child.
const serverProc = spawn(process.execPath, [fileURLToPath(import.meta.url), '--serve-fixtures'], { stdio: ['ignore', 'pipe', 'inherit'] });
const BASE = `http://127.0.0.1:${await new Promise((r) => serverProc.stdout.once('data', (d) => r(String(d).trim())))}`;

function node(script, scriptArgs, env = {}, seconds = 60) {
  const res = spawnSync(process.execPath, [join(ROOT, script), ...scriptArgs], { cwd: WORK, env: { ...process.env, ...env }, encoding: 'utf8', timeout: seconds * 1000 });
  const failure = res.error ? `\n[SUITE] spawn error: ${res.error.message}` : res.signal ? `\n[SUITE] killed by ${res.signal} after ${seconds}s` : '';
  return { code: res.status, out: `${res.stdout ?? ''}${res.stderr ?? ''}${failure}` };
}
const check = (name, env = {}, target = ['--target-url', BASE, '--no-reload']) =>
  node('scripts/cdp-sandbox.mjs', [join(ROOT, 'scripts', 'cdp-checks', name), '--port', CDP_PORT, '--keep-tab', ...target], env);
const success = result => { if (result.code !== 0) throw new Error(result.out); return result; };
const open = (name, path, env = {}) => success(check(name, env, ['--new-tab', `${BASE}/${path}`]));

// ref of the first snapshot row whose role+label matches, e.g. ref(out, 'button "More"')
function ref(out, label) {
  const line = out.split('\n').find((l) => /^\[SNAPSHOT\] \[e\d+\] /.test(l) && l.includes(`] ${label}`));
  if (!line) throw new Error(`no snapshot row for ${label}`);
  return line.match(/\[(e\d+)\]/)[1];
}
const snapshot = () => success(check('page-snapshot.mjs')).out;
const act = (env) => success(check('dom-operations-check.mjs', env)).out;
const waitFor = (text) => act({ DOM_ACTION: 'wait', DOM_VALUE: text, DOM_WAIT_MS: '4000' });

const results = [];
async function test(name, fn) {
  if (ONLY && !name.includes(ONLY)) return;
  const started = Date.now();
  try {
    await fn();
    results.push({ name, ok: true, ms: Date.now() - started });
    console.log(`ok   ${name} (${Date.now() - started}ms)`);
  } catch (error) {
    results.push({ name, ok: false, error: error.message });
    console.log(`FAIL ${name}: ${error.message}`);
  }
}
function expect(out, pattern, what) {
  if (!pattern.test(out)) throw new Error(`${what ?? pattern} not found in output:\n${out.split('\n').filter((l) => /^\[(ACTION|VERIFY|FINDING|WAIT|NEW|DIFF|SNAPSHOT|METRIC|SCREENSHOT)/.test(l)).slice(0, 30).join('\n')}`);
}
function refute(out, pattern, what) {
  if (pattern.test(out)) throw new Error(`unexpected ${what ?? pattern}`);
}

// The late-hero fixture needs a 1080px viewport to keep its candidate visible.
const launch = node('scripts/open-browser.mjs', ['--headless', '--windowSize', '1920x1080', '--port', CDP_PORT, '--url', 'about:blank'], {}, 60);
if (!/BROWSER_READY/.test(launch.out)) {
  console.log(launch.out);
  serverProc.kill();
  process.exit(1);
}

try {
  await test('events: trusted typing and synthetic fallback are distinguished', () => {
    open('page-snapshot.mjs', 'async-flow.html');
    for (const mode of ['trusted', 'js']) {
      const result = act({ DOM_ROLE: 'textbox', DOM_NAME: 'Shadow search', DOM_ACTION: 'type', DOM_VALUE: 'event', DOM_INPUT: mode, DOM_TRACE_EVENTS: '1' });
      const artifact = result.match(/\[ARTIFACT\] DOM_CHECK (.+)/)?.[1];
      const data = JSON.parse(readFileSync(artifact));
      const input = data.after.events.filter(e => e.type === 'input');
      if (!input.length || input.some(e => e.trusted !== (mode === 'trusted'))) throw new Error(`wrong event trust for ${mode}`);
      if (mode === 'trusted' && !data.after.events.some(e => e.type === 'keydown')) throw new Error('trusted key events missing');
    }
  });

  await test('wait: late selector and exact role/name after network content', () => {
    open('page-snapshot.mjs', 'async-flow.html');
    act({ DOM_ROLE: 'button', DOM_NAME: 'Load content', DOM_ACTION: 'click' });
    const waited = act({ DOM_ACTION: 'wait', DOM_SELECTOR: '#dynamic-search', DOM_WAIT_MS: '4000' });
    expect(waited, /\[WAIT\].*found/);
    const typed = act({ DOM_ROLE: 'textbox', DOM_NAME: 'Search collection', DOM_ACTION: 'type', DOM_VALUE: 'smart search' });
    expect(typed, /\[VERIFY\].*ok/);
    const snap = snapshot();
    expect(snap, /textbox "Search collection"/);
  });
  await test('wait: disabled control becomes enabled with progress', () => {
    open('page-snapshot.mjs', 'async-flow.html');
    const result = act({ DOM_SELECTOR: '#ready', DOM_ACTION: 'click', DOM_WAIT_MS: '4000', DOM_WAIT_TEXT: 'Progress complete' });
    expect(result, /\[PROGRESS\].*disabled=true/);
    expect(result, /\[WAIT\].*Progress complete/);
  });
  await test('wait: unmet post-action text fails', () => {
    open('page-snapshot.mjs', 'async-flow.html');
    const result = check('dom-operations-check.mjs', { DOM_SELECTOR: '#load', DOM_ACTION: 'click', DOM_WAIT_TEXT: 'will never appear', DOM_WAIT_MS: '600' });
    if (result.code === 0) throw new Error('post-action timeout reported success');
    expect(result.out, /WAIT_TIMEOUT/);
  });
  await test('find: ambiguous role/name does not choose a control', () => {
    open('page-snapshot.mjs', 'actions.html');
    const result = check('dom-operations-check.mjs', { DOM_ROLE: 'link', DOM_NAME: 'Repeated nav label', DOM_ACTION: 'click' });
    if (result.code === 0) throw new Error('ambiguous target was clicked');
    expect(result.out, /2 elements match role\/name/);
  });
  await test('snapshot: waits for delayed requested content', () => {
    open('page-snapshot.mjs', 'async-flow.html');
    act({ DOM_SELECTOR: '#load', DOM_ACTION: 'click', DOM_SETTLE_MS: '0' });
    const result = success(check('page-snapshot.mjs', { SNAPSHOT_WAIT_SELECTOR: '#dynamic-search', SNAPSHOT_WAIT_TEXT: 'Content loaded', SNAPSHOT_WAIT_MS: '4000' })).out;
    expect(result, /textbox "Search collection"/);
    refute(result, /PAGE_NOT_FULLY_LOADED/);
  });
  await test('network: redirects, full body timing and unfinished requests retained', () => {
    open('page-snapshot.mjs', 'async-flow.html');
    const result = success(check('network-measure-check.mjs', { MEASURE_URL: `${BASE}/network-flow`, NET_WAIT_MS: '1200', NET_SLOW_MS: '400' })).out;
    const artifact = result.match(/\[ARTIFACT\] NETWORK_MEASURE (.+)/)?.[1];
    const data = JSON.parse(readFileSync(artifact));
    const redirect = data.sample.find(r => r.url.endsWith('/redirect'));
    const streamed = data.sample.find(r => r.url.endsWith('/stream'));
    const pending = data.sample.find(r => r.url.endsWith('/pending'));
    if (redirect?.status !== 302 || !redirect.complete) throw new Error('redirect lost');
    if (!streamed?.complete || streamed.ms < 650) throw new Error(`body duration lost: ${streamed?.ms}`);
    if (!pending || pending.complete || data.health !== null) throw new Error('pending request hidden or scored healthy');
  });
  await test('events: input action and network captured in one flow', () => {
    open('page-snapshot.mjs', 'async-flow.html');
    const script = join(WORK, '.octocode', 'combined.mjs');
    writeFileSync(script, `import { run as monitor } from ${JSON.stringify('file://' + join(ROOT, 'scripts/cdp-checks/live-har-monitor.mjs'))};
import { run as act } from ${JSON.stringify('file://' + join(ROOT, 'scripts/cdp-checks/dom-operations-check.mjs'))};
export async function run(cdp) { await monitor(cdp, { onReady: () => act(cdp) }); }`);
    const result = success(node('scripts/cdp-runner.mjs', [script, '--port', CDP_PORT, '--target-url', BASE, '--keep-tab'], { DOM_ROLE: 'button', DOM_NAME: 'Load content', DOM_ACTION: 'click', DOM_WAIT_TEXT: 'Content loaded', DOM_TRACE_EVENTS: '1', MONITOR_MS: '1000' })).out;
    const dom = JSON.parse(readFileSync(result.match(/\[ARTIFACT\] DOM_CHECK (.+)/)[1]));
    const har = JSON.parse(readFileSync(result.match(/\[ARTIFACT\] HAR (.+)/)[1]));
    const click = dom.after.events.find(e => e.type === 'click' && e.trusted);
    const request = har.log.entries.find(e => e.request.url.endsWith('/redirect'));
    const streamed = har.log.entries.find(e => e.request.url.endsWith('/stream'));
    if (!click || !request || !streamed || streamed._pending) throw new Error('interaction/network evidence missing');
    if (click.wallTime > Date.parse(request.startedDateTime) + 100) throw new Error('event/request timeline does not align');
  });

  await test('network: isolated iframe events and response bodies', () => {
    open('page-snapshot.mjs', 'async-flow.html');
    const result = success(check('network-measure-check.mjs', { MEASURE_URL: `${BASE}/frame-network-flow`, NET_WAIT_MS: '1800' })).out;
    const data = JSON.parse(readFileSync(result.match(/\[ARTIFACT\] NETWORK_MEASURE (.+)/)[1]));
    const streamed = data.sample.find(r => r.url.endsWith('/stream'));
    if (!streamed?.sessionId || !streamed.complete) throw new Error('isolated iframe network request missing');
    const captured = success(check('network-body-har-fetch-check.mjs', { BODY_URL: `${BASE}/frame-network-flow`, BODY_MATCH: '/stream', BODY_WAIT_MS: '1800' })).out;
    const bodies = JSON.parse(readFileSync(captured.match(/\[ARTIFACT\] NETWORK_BODIES (.+)/)[1]));
    if (!bodies.some(b => b.body === 'Content loaded' && b.sessionId)) throw new Error('iframe response body missing');
  });

  await test('iframe: cross-origin target snapshot and trusted input', () => {
    const parent = open('page-snapshot.mjs', 'cross-frame').out;
    expect(parent, /Cross-origin frame/);
    const target = ['--target-url', `http://localhost:${new URL(BASE).port}/cross-frame.html`, '--target-type', 'iframe', '--no-reload'];
    const snap = success(check('page-snapshot.mjs', {}, target)).out;
    const result = success(check('dom-operations-check.mjs', { DOM_REF: ref(snap, 'textbox "Card number"'), DOM_ACTION: 'type', DOM_VALUE: '4242' }, target)).out;
    expect(result, /\[VERIFY\].*ok/);
    const paid = success(check('dom-operations-check.mjs', { DOM_ROLE: 'button', DOM_NAME: 'Pay now', DOM_ACTION: 'click', DOM_WAIT_TEXT: 'Paid in frame', DOM_WAIT_MS: '2000' }, target)).out;
    expect(paid, /\[WAIT\] found/);
  });

  await test('advanced: live protocol, browser sessions and trace streams', () => {
    open('page-snapshot.mjs', 'actions.html');
    const script = join(WORK, '.octocode', 'advanced.mjs');
    writeFileSync(script, `import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
export async function run(cdp) {
  const protocol = await cdp.protocol();
  if (protocol.domains.length < 30) throw new Error('Incomplete live protocol');
  cdp.saveArtifact('protocol.json', protocol);
  await cdp.send('SystemInfo.getInfo');
  const targets = await cdp.send('Target.getTargets');
  const page = targets.targetInfos.find(t => t.type === 'page');
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId: page.targetId, flatten: true });
  try {
    const value = await cdp.send('Runtime.evaluate', { expression: '6 * 7', returnByValue: true }, sessionId);
    if (value.result.value !== 42) throw new Error('Child session routing failed');
  } finally { await cdp.send('Target.detachFromTarget', { sessionId }); }
  let handler, timer;
  const completed = new Promise((resolve, reject) => {
    handler = resolve; cdp.on('Tracing.tracingComplete', handler);
    timer = setTimeout(() => reject(new Error('Trace did not finish')), 5000);
  });
  try {
    await cdp.send('Tracing.start', { transferMode: 'ReturnAsStream', categories: 'devtools.timeline' });
    await cdp.send('Browser.getVersion'); await cdp.send('Tracing.end');
    const { stream } = await completed; const chunks = [];
    try { for (;;) { const row = await cdp.send('IO.read', { handle: stream, size: 4096 }); chunks.push(row.base64Encoded ? Buffer.from(row.data, 'base64') : Buffer.from(row.data)); if (row.eof) break; } }
    finally { await cdp.send('IO.close', { handle: stream }); }
    const trace = Buffer.concat(chunks).toString('utf8');
    if (!Array.isArray(JSON.parse(trace).traceEvents)) throw new Error('Trace stream incomplete');
    cdp.saveArtifact('trace.json', JSON.parse(trace));
    try { cdp.saveArtifact('trace.json', {}); throw new Error('Overwrite accepted'); }
    catch (error) { if (!error.message.includes('Artifact exists')) throw error; }
  } finally { clearTimeout(timer); cdp.off('Tracing.tracingComplete', handler); }
}`);
    const result = success(node('scripts/cdp-sandbox.mjs', [script, '--port', CDP_PORT, '--browser'])).out;
    expect(result, /\[ARTIFACT\] trace.json/);
    expect(result, /\[NEXT\]/);
  });

  await test('emulation: opt-in patches pass without duplicate application', () => {
    const result = check('stealth-check.mjs', { STEALTH_CHECK_URL: `${BASE}/actions.html` }, ['--new-tab', 'about:blank', '--stealth']);
    if (result.code !== 0) throw new Error(result.out);
    expect(result.out, /stealth self-test: 15\/15 passed/);
  });
  await test('runner: interruption is recorded and exits nonzero', async () => {
    const script = join(WORK, '.octocode', 'interrupt.mjs');
    writeFileSync(script, "export async function run() { console.log('READY_FOR_SIGNAL'); await new Promise(() => {}); }");
    const child = spawn(process.execPath, [join(ROOT, 'scripts/cdp-runner.mjs'), script, '--port', CDP_PORT, '--new-tab', 'about:blank'], { cwd: WORK, stdio: ['ignore', 'pipe', 'pipe'] });
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('interrupt check timed out')); }, 10000);
      child.stdout.on('data', data => { if (String(data).includes('READY_FOR_SIGNAL')) child.kill('SIGTERM'); });
      child.on('exit', code => { clearTimeout(timer); if (code !== 143) reject(new Error(`expected 143, got ${code}`)); else resolve(); });
      child.on('error', reject);
    });
    const metadata = JSON.parse(readFileSync(join(WORK, '.octocode/tmp/chrome-devtools/session-meta', 'port-' + CDP_PORT, 'session-metadata.json'), 'utf8'));
    if (metadata.lastRunStatus !== 'interrupted') throw new Error('interrupt metadata missing');
  });

  await test('runner: failed action exits nonzero and records error', () => {
    open('page-snapshot.mjs', 'actions.html');
    const result = check('dom-operations-check.mjs', { DOM_SELECTOR: '#missing-element', DOM_ACTION: 'click' });
    if (result.code === 0) throw new Error('missing target reported success');
    const metadata = JSON.parse(readFileSync(join(WORK, '.octocode/tmp/chrome-devtools/session-meta', 'port-' + CDP_PORT, 'session-metadata.json'), 'utf8'));
    if (metadata.lastRunStatus !== 'error') throw new Error('failed action metadata is not error');
  });

  await test('snapshot: identical labels preserve distinct targets', () => {
    const out = open('page-snapshot.mjs', 'actions.html').out;
    const rows = out.split('\n').filter(l => /\] link "Repeated nav label"/.test(l));
    if (rows.length !== 2) throw new Error(`expected both controls, got ${rows.length}`);
  });
  await test('screenshot: missing element fails', () => {
    open('page-snapshot.mjs', 'actions.html');
    const result = check('page-screenshot.mjs', { SHOT_SELECTOR: '#missing-element' });
    if (result.code === 0) throw new Error('missing screenshot element reported success');
    expect(result.out, /SHOT_SELECTOR_MISSING/);
  });

  await test('snapshot: clickables, iframe inlined, wrapper dropped', () => {
    const out = open('page-snapshot.mjs', 'actions.html').out;
    expect(out, /\] clickable ~"Open card"/);
    expect(out, /\] clickable ~"Listener card"/);
    expect(out, /\] link "Linked card"/);
    refute(out, /clickable ~"Linked card"/, 'clickable wrapper around a link');
    expect(out, /— iframe .*\/frame\.html/);
    expect(out, /\] textbox "Card number"/);
    expect(out, /\] button "Upload"|\] button "Choose File"/, 'file input row');
  });

  await test('click: non-semantic clickable', () => {
    const out = act({ DOM_REF: ref(snapshot(), 'clickable ~"Open card"'), DOM_ACTION: 'click' });
    expect(out, /\[ACTION\] clicked "Open card"/);
    expect(waitFor('card:1'), /\[WAIT\] found/);
  });

  await test('fill: full read-back for long input', () => {
    const value = 'long input '.repeat(50);
    const out = act({ DOM_REF: ref(snapshot(), 'textbox "Name"'), DOM_ACTION: 'fill', DOM_VALUE: value });
    expect(out, /\[VERIFY\] ok/);
    refute(out, /VERIFY_MISMATCH/);
    expect(out, new RegExp(JSON.stringify(value)), 'complete input read-back');
  });

  await test('steps: fill + select + check in one run', () => {
    const snap = snapshot();
    const out = act({ DOM_STEPS: JSON.stringify([
      { ref: ref(snap, 'textbox "Name"'), action: 'fill', value: 'Ada' },
      { ref: ref(snap, 'combobox "Fruit"'), action: 'select', value: 'Banana' },
      { ref: ref(snap, 'checkbox "Agree"'), action: 'check' },
    ]) });
    expect(out, /\[VERIFY\] #1 ok .*"value":"Ada"/);
    expect(out, /\[VERIFY\] #2 ok .*"selected":"Banana"/);
    expect(out, /\[VERIFY\] #3 ok .*"checked":true/);
    expect(out, /\[METRIC\] STEPS done=3\/3/);
  });

  await test('hover: diff names the revealed menu item', () => {
    const out = act({ DOM_REF: ref(snapshot(), 'button "More"'), DOM_ACTION: 'hover' });
    expect(out, /\[NEW\] \[e\d+\] link "Hidden item"/);
    refute(out, /NO_VISIBLE_EFFECT/, 'NO_VISIBLE_EFFECT on a revealing hover');
  });

  await test('wait: async content after click', () => {
    open('page-snapshot.mjs', 'actions.html');
    const out = act({ DOM_REF: ref(snapshot(), 'button "Load later"'), DOM_ACTION: 'click', DOM_WAIT_TEXT: 'Loaded!' });
    expect(out, /\[WAIT\] found "Loaded!" after \d+ms/);
  });

  await test('dblclick: trusted', () => {
    const out = act({ DOM_REF: ref(snapshot(), 'button "Double me"'), DOM_ACTION: 'dblclick' });
    expect(out, /\[ACTION\] double-clicked "Double me"/);
    expect(`${out}\n${waitFor('dbl:true')}`, /\[WAIT\] found/);
  });

  await test('upload: hidden file input', () => {
    const dir = mkdtempSync(join(tmpdir(), 'octo-upload-'));
    const file = join(dir, 'report.txt');
    writeFileSync(file, 'hello\n');
    const out = act({ DOM_SELECTOR: '#file', DOM_ACTION: 'upload', DOM_VALUE: file });
    rmSync(dir, { recursive: true, force: true });
    expect(out, /\[VERIFY\] ok .*"files":"report\.txt"/);
  });

  await test('drag: pointer events', () => {
    act({ DOM_REF: ref(snapshot(), 'button "Drag handle"'), DOM_ACTION: 'drag', DOM_TO_SELECTOR: '#zone' });
    expect(waitFor('dropped:zone'), /\[WAIT\] found/);
  });

  await test('drag: HTML5 drag-and-drop', () => {
    const out = act({ DOM_REF: ref(snapshot(), 'button "HTML5 source"'), DOM_ACTION: 'drag', DOM_TO_SELECTOR: '#zone5' });
    expect(out, /\(html5\)/, 'html5 drag kind');
    expect(waitFor('html5:payload'), /\[WAIT\] found/);
  });

  await test('iframe: type and click inside a same-origin frame', () => {
    const snap = snapshot();
    const out = act({ DOM_STEPS: JSON.stringify([
      { ref: ref(snap, 'textbox "Card number"'), action: 'type', value: '4242' },
      { ref: ref(snap, 'button "Pay now"'), action: 'click' },
    ]) });
    expect(out, /\[VERIFY\] #1 ok/);
    expect(waitFor('card:4242'), /\[WAIT\] found/);
    expect(waitFor('paid:true'), /\[WAIT\] found/);
  });

  await test('screenshot: annotated refs', () => {
    open('page-snapshot.mjs', 'actions.html');
    const out = success(check('page-screenshot.mjs', { SHOT_ANNOTATE: '1' })).out;
    const labels = Number(out.match(/labels=(\d+)/)?.[1] ?? 0);
    if (labels < 5) throw new Error(`expected >=5 labels, got ${labels}`);
    const file = out.match(/\[SCREENSHOT\] (\S+)/)?.[1];
    if (!file || !existsSync(file)) throw new Error('screenshot file missing');
  });

  await test('screenshot: long page retains every tile', () => {
    const url = 'data:text/html,<body style="margin:0;height:17000px;background:linear-gradient(red,blue)">Long page</body>';
    const result = success(check('page-screenshot.mjs', { SHOT_FULL: '1', SHOT_SCALE: '0.25' }, ['--new-tab', url])).out;
    const path = result.match(/\[ARTIFACT\] screenshot-manifest.json (.+)/)?.[1];
    const manifest = JSON.parse(readFileSync(path));
    if (!manifest.complete || manifest.tiles.length !== 3) throw new Error('Long screenshot is incomplete');
    let y = 0;
    for (const tile of manifest.tiles) {
      if (tile.clip.y !== y || !readFileSync(tile.file).length) throw new Error('Screenshot coverage gap');
      y += tile.clip.height;
    }
    if (y !== 17000) throw new Error('Screenshot height was truncated');
    refute(result, /SHOT_TRUNCATED/);
  });

  await test('press: Enter submits and navigates', () => {
    open('page-snapshot.mjs', 'actions.html');
    act({ DOM_SELECTOR: '#name', DOM_ACTION: 'fill', DOM_VALUE: 'Ada' });
    const snap = snapshot();
    const out = act({ DOM_REF: ref(snap, 'textbox "Name"'), DOM_ACTION: 'press', DOM_KEY: 'Enter' });
    expect(out, /navigated=.*\/done\?q=Ada/);
    expect(out, /\[NEXT\] page navigated/);
  });

  await test('snapshot: list context and unnamed links', () => {
    const out = open('page-snapshot.mjs', 'list.html').out;
    expect(out, /— .*128 points by ada/);
    expect(out, /\] link "upvote"/);
    expect(out, /\] link ~"votearrow/);
  });

  await test('perf: LCP and CLS from a late hero and banner', () => {
    const result = check('performance-measure-check.mjs', { MEASURE_URL: `${BASE}/perf.html`, PERF_WAIT_MS: '3500' }, ['--new-tab', 'about:blank']);
    if (result.code !== 0) throw new Error(result.out);
    const out = result.out;
    const lcp = Number(out.match(/lcp=(\d+)/)?.[1] ?? 0);
    const cls = Number(out.match(/cls=([\d.]+)/)?.[1] ?? 0);
    if (lcp < 2500) throw new Error(`expected lcp >= 2500, got ${lcp}`);
    if (cls < 0.05) throw new Error(`expected cls >= 0.05, got ${cls}`);
    expect(out, /\[FINDING\] PERF_SLOW_LCP|SLOW_LCP/);
  });
} finally {
  serverProc.kill();
  if (!KEEP) {
    node('scripts/open-browser.mjs', ['--cleanup', '--port', CDP_PORT], {}, 30);
    rmSync(WORK, { recursive: true, force: true });
  } else console.log(`kept: port ${CDP_PORT}, artifacts in ${WORK}`);
}

const failed = results.filter((r) => !r.ok);
console.log(JSON.stringify({ ok: failed.length === 0, suite: 'chrome-devtools-live', passed: results.length - failed.length, failed: failed.length }));
process.exit(failed.length ? 1 : 0);
