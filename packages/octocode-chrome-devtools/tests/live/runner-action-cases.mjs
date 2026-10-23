// Runner exit metadata, snapshot rows and single DOM actions on the actions fixture.
import { spawn } from 'node:child_process';
import { writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import {
  live,
  check,
  open,
  ref,
  snapshot,
  act,
  waitFor,
  expect,
  refute,
  sessionMeta,
} from './harness.mjs';

export const cases = [];
const test = (name, fn) => cases.push([name, fn]);

test('runner: interruption is recorded and exits nonzero', async () => {
  const script = join(live.WORK, '.octocode', 'interrupt.mjs');
  writeFileSync(
    script,
    "export async function run() { console.log('READY_FOR_SIGNAL'); await new Promise(() => {}); }"
  );
  const child = spawn(
    process.execPath,
    [
      join(live.ROOT, 'dist/engine/cdp-runner.mjs'),
      script,
      '--port',
      live.CDP_PORT,
      '--new-tab',
      'about:blank',
    ],
    { cwd: live.WORK, stdio: ['ignore', 'pipe', 'pipe'] }
  );
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error('interrupt check timed out'));
    }, 10000);
    child.stdout.on('data', data => {
      if (String(data).includes('READY_FOR_SIGNAL')) child.kill('SIGTERM');
    });
    child.on('exit', code => {
      clearTimeout(timer);
      if (code !== 143) reject(new Error(`expected 143, got ${code}`));
      else resolve();
    });
    child.on('error', reject);
  });
  const metadata = sessionMeta('session-metadata.json');
  if (metadata.lastRunStatus !== 'interrupted')
    throw new Error('interrupt metadata missing');
});

test('runner: failed action exits nonzero and records error', () => {
  open('page-snapshot.mjs', 'actions.html');
  const result = check('dom-operations-check.mjs', {
    DOM_SELECTOR: '#missing-element',
    DOM_ACTION: 'click',
  });
  if (result.code === 0) throw new Error('missing target reported success');
  const metadata = sessionMeta('session-metadata.json');
  if (metadata.lastRunStatus !== 'error')
    throw new Error('failed action metadata is not error');
});

test('snapshot: identical labels preserve distinct targets', () => {
  const out = open('page-snapshot.mjs', 'actions.html').out;
  const rows = out
    .split('\n')
    .filter(l => /\] link "Repeated nav label"/.test(l));
  if (rows.length !== 2)
    throw new Error(`expected both controls, got ${rows.length}`);
});
test('screenshot: missing element fails', () => {
  open('page-snapshot.mjs', 'actions.html');
  const result = check('page-screenshot.mjs', {
    SHOT_SELECTOR: '#missing-element',
  });
  if (result.code === 0)
    throw new Error('missing screenshot element reported success');
  expect(result.out, /SHOT_SELECTOR_MISSING/);
});

test('snapshot: clickables, iframe inlined, wrapper dropped', () => {
  const out = open('page-snapshot.mjs', 'actions.html').out;
  expect(out, /\] clickable ~"Open card"/);
  expect(out, /\] clickable ~"Listener card"/);
  expect(out, /\] link "Linked card"/);
  refute(out, /clickable ~"Linked card"/, 'clickable wrapper around a link');
  expect(out, /— iframe .*\/frame\.html/);
  expect(out, /\] textbox "Card number"/);
  expect(out, /\] button "Upload"|\] button "Choose File"/, 'file input row');
});

test('click: non-semantic clickable', () => {
  const out = act({
    DOM_REF: ref(snapshot(), 'clickable ~"Open card"'),
    DOM_ACTION: 'click',
  });
  expect(out, /\[ACTION\] clicked "Open card"/);
  expect(waitFor('card:1'), /\[WAIT\] found/);
});

test('fill: full read-back for long input', () => {
  const value = 'long input '.repeat(50);
  const out = act({
    DOM_REF: ref(snapshot(), 'textbox "Name"'),
    DOM_ACTION: 'fill',
    DOM_VALUE: value,
  });
  expect(out, /\[VERIFY\] ok/);
  refute(out, /VERIFY_MISMATCH/);
  expect(out, new RegExp(JSON.stringify(value)), 'complete input read-back');
});

test('steps: fill + select + check in one run', () => {
  open('page-snapshot.mjs', 'actions.html');
  const snap = snapshot();
  const out = act({
    DOM_STEPS: JSON.stringify([
      { ref: ref(snap, 'textbox "Name"'), action: 'fill', value: 'Ada' },
      {
        ref: ref(snap, 'combobox "Fruit"'),
        action: 'select',
        value: 'Banana',
      },
      { ref: ref(snap, 'checkbox "Agree"'), action: 'check' },
    ]),
  });
  expect(out, /\[VERIFY\] #1 ok .*"value":"Ada"/);
  expect(out, /\[VERIFY\] #2 ok .*"selected":"Banana"/);
  expect(out, /\[VERIFY\] #3 ok .*"checked":true/);
  expect(out, /\[METRIC\] STEPS done=3\/3/);
});

test('hover: diff names the revealed menu item', () => {
  const out = act({
    DOM_REF: ref(snapshot(), 'button "More"'),
    DOM_ACTION: 'hover',
  });
  expect(out, /\[NEW\] \[e\d+\] link "Hidden item"/);
  refute(out, /NO_VISIBLE_EFFECT/, 'NO_VISIBLE_EFFECT on a revealing hover');
});

test('wait: async content after click', () => {
  open('page-snapshot.mjs', 'actions.html');
  const out = act({
    DOM_REF: ref(snapshot(), 'button "Load later"'),
    DOM_ACTION: 'click',
    DOM_WAIT_TEXT: 'Loaded!',
  });
  expect(out, /\[WAIT\] found "Loaded!" after \d+ms/);
});

test('dblclick: trusted', () => {
  const out = act({
    DOM_REF: ref(snapshot(), 'button "Double me"'),
    DOM_ACTION: 'dblclick',
  });
  expect(out, /\[ACTION\] double-clicked "Double me"/);
  expect(`${out}\n${waitFor('dbl:true')}`, /\[WAIT\] found/);
});

test('upload: hidden file input', () => {
  const dir = mkdtempSync(join(tmpdir(), 'octo-upload-'));
  const file = join(dir, 'report.txt');
  writeFileSync(file, 'hello\n');
  const out = act({
    DOM_SELECTOR: '#file',
    DOM_ACTION: 'upload',
    DOM_VALUE: file,
  });
  rmSync(dir, { recursive: true, force: true });
  expect(out, /\[VERIFY\] ok .*"files":"report\.txt"/);
});

test('drag: pointer events', () => {
  act({
    DOM_REF: ref(snapshot(), 'button "Drag handle"'),
    DOM_ACTION: 'drag',
    DOM_TO_SELECTOR: '#zone',
  });
  expect(waitFor('dropped:zone'), /\[WAIT\] found/);
});

test('drag: HTML5 drag-and-drop', () => {
  const out = act({
    DOM_REF: ref(snapshot(), 'button "HTML5 source"'),
    DOM_ACTION: 'drag',
    DOM_TO_SELECTOR: '#zone5',
  });
  expect(out, /\(html5\)/, 'html5 drag kind');
  expect(waitFor('html5:payload'), /\[WAIT\] found/);
});

test('iframe: type and click inside a same-origin frame', () => {
  const snap = snapshot();
  const out = act({
    DOM_STEPS: JSON.stringify([
      {
        ref: ref(snap, 'textbox "Card number"'),
        action: 'type',
        value: '4242',
      },
      { ref: ref(snap, 'button "Pay now"'), action: 'click' },
    ]),
  });
  expect(out, /\[VERIFY\] #1 ok/);
  expect(waitFor('card:4242'), /\[WAIT\] found/);
  expect(waitFor('paid:true'), /\[WAIT\] found/);
});
