#!/usr/bin/env node
// Live regression suite: local fixtures + isolated headless Chrome + the real check scripts.
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

const server = createServer((req, res) => {
  const url = new URL(req.url, 'http://fixture');
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
  const res = spawnSync(process.execPath, [script, ...scriptArgs], { cwd: ROOT, env: { ...process.env, ...env }, encoding: 'utf8', timeout: seconds * 1000 });
  return { code: res.status, out: `${res.stdout ?? ''}${res.stderr ?? ''}` };
}
const check = (name, env = {}, target = ['--target-url', BASE, '--no-reload']) =>
  node('scripts/cdp-sandbox.mjs', [`scripts/cdp-checks/${name}`, '--port', CDP_PORT, '--keep-tab', ...target], env);
const open = (name, path, env = {}) => check(name, env, ['--new-tab', `${BASE}/${path}`]);

// ref of the first snapshot row whose role+label matches, e.g. ref(out, 'button "More"')
function ref(out, label) {
  const line = out.split('\n').find((l) => /^\[SNAPSHOT\] \[e\d+\] /.test(l) && l.includes(`] ${label}`));
  if (!line) throw new Error(`no snapshot row for ${label}`);
  return line.match(/\[(e\d+)\]/)[1];
}
const snapshot = () => check('page-snapshot.mjs').out;
const act = (env) => check('dom-operations-check.mjs', env).out;
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

const launch = node('scripts/open-browser.mjs', ['--headless', '--port', CDP_PORT, '--url', 'about:blank'], {}, 60);
if (!/BROWSER_READY/.test(launch.out)) {
  console.log(launch.out);
  serverProc.kill();
  process.exit(1);
}

try {
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
    const out = act({ DOM_REF: ref(snapshot(), 'button "Load later"'), DOM_ACTION: 'click', DOM_WAIT_TEXT: 'Loaded!' });
    expect(out, /\[WAIT\] found "Loaded!" after \d+ms/);
  });

  await test('dblclick: trusted', () => {
    act({ DOM_REF: ref(snapshot(), 'button "Double me"'), DOM_ACTION: 'dblclick' });
    expect(waitFor('dbl:true'), /\[WAIT\] found/);
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
    snapshot();
    const out = check('page-screenshot.mjs', { SHOT_ANNOTATE: '1' }).out;
    const labels = Number(out.match(/labels=(\d+)/)?.[1] ?? 0);
    if (labels < 5) throw new Error(`expected >=5 labels, got ${labels}`);
    const file = out.match(/\[SCREENSHOT\] (\S+)/)?.[1];
    if (!file || !existsSync(file)) throw new Error('screenshot file missing');
  });

  await test('press: Enter submits and navigates', () => {
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
    const out = check('performance-measure-check.mjs', { MEASURE_URL: `${BASE}/perf.html`, PERF_WAIT_MS: '3500' }).out;
    const lcp = Number(out.match(/lcp=(\d+)/)?.[1] ?? 0);
    const cls = Number(out.match(/cls=([\d.]+)/)?.[1] ?? 0);
    if (lcp < 2500) throw new Error(`expected lcp >= 2500, got ${lcp}`);
    if (cls < 0.05) throw new Error(`expected cls >= 0.05, got ${cls}`);
    expect(out, /\[FINDING\] PERF_SLOW_LCP|SLOW_LCP/);
  });
} finally {
  serverProc.kill();
  if (!KEEP) node('scripts/open-browser.mjs', ['--cleanup', '--port', CDP_PORT], {}, 30);
}

const failed = results.filter((r) => !r.ok);
console.log(JSON.stringify({ ok: failed.length === 0, suite: 'chrome-devtools-live', passed: results.length - failed.length, failed: failed.length }));
process.exit(failed.length ? 1 : 0);
