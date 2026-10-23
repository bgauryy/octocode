#!/usr/bin/env node
// Live regression suite: local fixtures + isolated headless Chrome + the real check scripts.
// Fixtures: tests/fixtures/actions.html (+ tests/fixtures/frame.html iframe),
// tests/fixtures/list.html, tests/fixtures/perf.html.
// Needs Chrome; no network. Usage: node tests/live-suite.mjs [--keep] [--only <name-substring[,name-substring...]>]
// Modules under live/: fixture-server (HTTP fixtures), harness (runners, readers and
// assertions) and *-cases (named cases in suite order).
import { spawn } from 'child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'fs';
import { dirname, join } from 'path';
import { tmpdir } from 'os';
import { fileURLToPath } from 'url';
import { createFixtureServer } from './live/fixture-server.mjs';
import {
  live,
  results,
  node,
  check,
  success,
  open,
  runCase,
} from './live/harness.mjs';
import { cases as cliCases } from './live/cli-cases.mjs';
import { cases as waitSnapshotCases } from './live/wait-snapshot-cases.mjs';
import { cases as networkFrameCases } from './live/network-frame-cases.mjs';
import { cases as runnerActionCases } from './live/runner-action-cases.mjs';
import { cases as executorCases } from './live/executor-cases.mjs';
import { cases as pageCases } from './live/page-cases.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const FIXTURES = join(ROOT, 'tests', 'fixtures');
const args = process.argv.slice(2);
if (args.includes('-h') || args.includes('--help')) {
  console.log(
    'Usage: live-suite.mjs [--keep] [--only <substring[,substring...]>]\n\nRuns snapshot, action, wait, upload, drag, iframe, screenshot, perf and cross-cwd cleanup checks against local fixtures in an isolated headless Chrome.'
  );
  process.exit(0);
}
const SERVE = args.includes('--serve-fixtures'); // internal: child process that serves the fixtures
const KEEP = args.includes('--keep');
// A comma-separated list keeps every case matching any substring, in suite order.
const ONLY = args.includes('--only')
  ? args[args.indexOf('--only') + 1].split(',')
  : [];
const CDP_PORT = String(9400 + Math.floor(Math.random() * 400));
// Checks stage helpers and write artifacts under <cwd>/.octocode; keep that out of the skill folder.
const WORK = SERVE ? null : mkdtempSync(join(tmpdir(), 'octo-live-'));
if (SERVE) {
  const server = createFixtureServer(FIXTURES);
  server.listen(0, '127.0.0.1', () => console.log(server.address().port));
  await new Promise(() => {});
}
// The checks run through spawnSync, which blocks this event loop, so the server lives in a child.
const serverProc = spawn(
  process.execPath,
  [fileURLToPath(import.meta.url), '--serve-fixtures'],
  { stdio: ['ignore', 'pipe', 'inherit'] }
);
const BASE = `http://127.0.0.1:${await new Promise(r => serverProc.stdout.once('data', d => r(String(d).trim())))}`;
Object.assign(live, { ROOT, WORK, CDP_PORT, BASE, ONLY });

// HAR/network capture cases; they read the HAR's `_pending` extension field.
const networkCaptureCases = [];
const test = (name, fn) => networkCaptureCases.push([name, fn]);
test('network: redirects, full body timing and unfinished requests retained', () => {
  open('page-snapshot.mjs', 'async-flow.html');
  const result = success(
    check('network-measure-check.mjs', {
      MEASURE_URL: `${live.BASE}/network-flow`,
      NET_WAIT_MS: '1200',
      NET_SLOW_MS: '400',
    })
  ).out;
  const artifact = result.match(/\[ARTIFACT\] NETWORK_MEASURE (.+)/)?.[1];
  const data = JSON.parse(readFileSync(artifact));
  const redirect = data.sample.find(r => r.url.endsWith('/redirect'));
  const streamed = data.sample.find(r => r.url.endsWith('/stream'));
  const pending = data.sample.find(r => r.url.endsWith('/pending'));
  if (redirect?.status !== 302 || !redirect.complete)
    throw new Error('redirect lost');
  if (!streamed?.complete || streamed.ms < 650)
    throw new Error(`body duration lost: ${streamed?.ms}`);
  if (!pending || pending.complete || data.health !== null)
    throw new Error('pending request hidden or scored healthy');
});
test('events: input action and network captured in one flow', () => {
  open('page-snapshot.mjs', 'async-flow.html');
  const script = join(live.WORK, '.octocode', 'combined.mjs');
  writeFileSync(
    script,
    `import { run as monitor } from ${JSON.stringify('file://' + join(live.ROOT, 'dist/engine/cdp-checks/live-har-monitor.mjs'))};
import { run as act } from ${JSON.stringify('file://' + join(live.ROOT, 'dist/engine/cdp-checks/dom-operations-check.mjs'))};
export async function run(cdp) { await monitor(cdp, { onReady: () => act(cdp) }); }`
  );
  const result = success(
    node(
      'dist/engine/cdp-runner.mjs',
      [
        script,
        '--port',
        live.CDP_PORT,
        '--target-url',
        live.BASE,
        '--keep-tab',
      ],
      {
        DOM_ROLE: 'button',
        DOM_NAME: 'Load content',
        DOM_ACTION: 'click',
        DOM_WAIT_TEXT: 'Content loaded',
        DOM_TRACE_EVENTS: '1',
        MONITOR_MS: '1000',
      }
    )
  ).out;
  const dom = JSON.parse(
    readFileSync(result.match(/\[ARTIFACT\] DOM_CHECK (.+)/)[1])
  );
  const har = JSON.parse(
    readFileSync(result.match(/\[ARTIFACT\] HAR (.+)/)[1])
  );
  const click = dom.after.events.find(e => e.type === 'click' && e.trusted);
  const request = har.log.entries.find(e =>
    e.request.url.endsWith('/redirect')
  );
  const streamed = har.log.entries.find(e => e.request.url.endsWith('/stream'));
  if (!click || !request || !streamed || streamed._pending)
    throw new Error('interaction/network evidence missing');
  if (click.wallTime > Date.parse(request.startedDateTime) + 100)
    throw new Error('event/request timeline does not align');
});

// The late-hero fixture needs a 1080px viewport to keep its candidate visible.
const launch = node(
  'dist/engine/cli.mjs',
  [
    'open',
    '--headless',
    '--windowSize',
    '1920x1080',
    '--port',
    CDP_PORT,
    '--url',
    'about:blank',
  ],
  {},
  60
);
if (!/BROWSER_READY/.test(launch.out)) {
  console.log(launch.out);
  serverProc.kill();
  process.exit(1);
}

try {
  for (const [name, fn] of [
    ...cliCases,
    ...waitSnapshotCases,
    ...networkCaptureCases,
    ...networkFrameCases,
    ...runnerActionCases,
    ...executorCases,
    ...pageCases,
  ])
    await runCase(name, fn);
} finally {
  serverProc.kill();
  if (!KEEP) {
    node('dist/engine/cli.mjs', ['cleanup', '--port', CDP_PORT], {}, 30);
    rmSync(WORK, { recursive: true, force: true });
  } else console.log(`kept: port ${CDP_PORT}, artifacts in ${WORK}`);
}

const failed = results.filter(r => !r.ok);
console.log(
  JSON.stringify({
    ok: failed.length === 0,
    suite: 'chrome-devtools-live',
    passed: results.length - failed.length,
    failed: failed.length,
  })
);
process.exit(failed.length ? 1 : 0);
