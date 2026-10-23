// Isolated-frame network and input, live protocol streams and emulation reach.
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { live, node, check, success, open, ref, expect } from './harness.mjs';

export const cases = [];
const test = (name, fn) => cases.push([name, fn]);

test('network: isolated iframe events and response bodies', () => {
  open('page-snapshot.mjs', 'async-flow.html');
  const result = success(
    check('network-measure-check.mjs', {
      MEASURE_URL: `${live.BASE}/frame-network-flow`,
      NET_WAIT_MS: '1800',
    })
  ).out;
  const data = JSON.parse(
    readFileSync(result.match(/\[ARTIFACT\] NETWORK_MEASURE (.+)/)[1])
  );
  const streamed = data.sample.find(r => r.url.endsWith('/stream'));
  if (!streamed?.sessionId || !streamed.complete)
    throw new Error('isolated iframe network request missing');
  const captured = success(
    check('network-body-har-fetch-check.mjs', {
      BODY_URL: `${live.BASE}/frame-network-flow`,
      BODY_MATCH: '/stream',
      BODY_WAIT_MS: '1800',
    })
  ).out;
  const bodies = JSON.parse(
    readFileSync(captured.match(/\[ARTIFACT\] NETWORK_BODIES (.+)/)[1])
  );
  if (!bodies.some(b => b.body === 'Content loaded' && b.sessionId))
    throw new Error('iframe response body missing');
});

test('iframe: cross-origin target snapshot and trusted input', () => {
  const parent = open('page-snapshot.mjs', 'cross-frame').out;
  expect(parent, /Cross-origin frame/);
  const target = [
    '--target-url',
    `http://localhost:${new URL(live.BASE).port}/cross-frame.html`,
    '--target-type',
    'iframe',
    '--no-reload',
  ];
  open('page-snapshot.mjs', 'actions.html'); // The iframe's owning page is now in the background.
  const snap = success(check('page-snapshot.mjs', {}, target)).out;
  const result = success(
    check(
      'dom-operations-check.mjs',
      {
        DOM_REF: ref(snap, 'textbox "Card number"'),
        DOM_ACTION: 'type',
        DOM_VALUE: '4242',
      },
      target
    )
  ).out;
  expect(result, /\[VERIFY\].*ok/);
  const paid = success(
    check(
      'dom-operations-check.mjs',
      {
        DOM_ROLE: 'button',
        DOM_NAME: 'Pay now',
        DOM_ACTION: 'click',
        DOM_WAIT_TEXT: 'Paid in frame',
        DOM_WAIT_MS: '2000',
      },
      target
    )
  ).out;
  expect(paid, /\[WAIT\] found/);
});

test('advanced: live protocol, browser sessions and trace streams', () => {
  open('page-snapshot.mjs', 'actions.html');
  const script = join(live.WORK, '.octocode', 'advanced.mjs');
  writeFileSync(
    script,
    `import { writeFileSync } from 'node:fs';
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
  // The flush waits on every renderer: seconds on a loaded host, so the bound only catches a hang.
  handler = resolve; cdp.on('Tracing.tracingComplete', handler);
  timer = setTimeout(() => reject(new Error('Trace did not finish')), 30000);
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
}`
  );
  const result = success(
    node('dist/engine/cdp-sandbox.mjs', [
      script,
      '--port',
      live.CDP_PORT,
      '--browser',
    ])
  ).out;
  expect(result, /\[ARTIFACT\] trace.json/);
  expect(result, /\[NEXT\]/);
});

test('emulation: opt-in patches pass without duplicate application', () => {
  const result = check(
    'stealth-check.mjs',
    { STEALTH_CHECK_URL: `${live.BASE}/actions.html` },
    ['--new-tab', 'about:blank', '--stealth']
  );
  if (result.code !== 0) throw new Error(result.out);
  expect(result.out, /stealth self-test: (\d+)\/\1 passed/);
});
test('emulation: overrides reach cross-origin frames and workers, and end with the run', () => {
  const script = join(live.WORK, '.octocode', 'stealth-reach.mjs');
  writeFileSync(
    script,
    `export async function run(cdp) {
const frames = [];
cdp.on('Target.attachedToTarget', ({ sessionId, targetInfo }) => {
  if (targetInfo.type === 'iframe') frames.push(sessionId);
});
await cdp.send('Target.setAutoAttach', cdp.stealthAutoAttach ?? { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
await cdp.send('Page.reload', {});
await new Promise(r => setTimeout(r, 2000));
const page = (await cdp.send('Runtime.evaluate', { awaitPromise: true, returnByValue: true, expression:
  \`new Promise(r => { __worker.onmessage = e => r({ ua: navigator.userAgent, platform: navigator.platform, own: Object.getOwnPropertyNames(navigator), webdriver: navigator.webdriver, worker: e.data }); __worker.postMessage(0); })\` })).result.value;
let frame = null;
for (const sessionId of frames) {
  const value = (await cdp.send('Runtime.evaluate', { expression: 'window.__seen', returnByValue: true }, sessionId).catch(() => null))?.result?.value;
  if (value) frame = value;
}
console.log('[REACH] ' + JSON.stringify({ page, frame }));
}`
  );
  const run = extra =>
    JSON.parse(
      success(
        node('dist/engine/cdp-sandbox.mjs', [
          script,
          '--port',
          live.CDP_PORT,
          ...extra,
        ])
      )
        .out.split('\n')
        .find(line => line.startsWith('[REACH] '))
        .slice(8)
    );
  const stealthy = run([
    '--new-tab',
    `${live.BASE}/stealth-flow`,
    '--keep-tab',
    '--stealth',
  ]);
  const { page, frame } = stealthy;
  if (/Headless/.test(page.ua)) throw new Error(`page UA ${page.ua}`);
  if (page.webdriver !== false || page.own.length)
    throw new Error(`navigator tells ${JSON.stringify(page)}`);
  for (const [where, seen] of [
    ['worker', page.worker],
    ['frame', frame],
  ])
    if (!seen || seen.ua !== page.ua || seen.platform !== page.platform)
      throw new Error(`${where} differs: ${JSON.stringify(stealthy)}`);
  if (frame.header !== page.ua)
    throw new Error(`frame request header ${frame.header}`);
  // Nothing outlives the run: the kept tab is native and self-consistent again.
  const native = run([
    '--target-url',
    `${live.BASE}/stealth-flow`,
    '--close-tab',
  ]);
  if (
    native.page.worker.ua !== native.page.ua ||
    native.frame?.ua !== native.page.ua
  )
    throw new Error(`native run inconsistent: ${JSON.stringify(native)}`);
});
