// Engine CLI: generic plans, target selection, installed protocol and cancellation.
import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  live,
  success,
  cli,
  artifact,
  expect,
  sessionMeta,
} from './harness.mjs';

export const cases = [];
const test = (name, fn) => cases.push([name, fn]);
// Set by the first case; a focused run creates its own target.
let cliTarget;

test('cli: generic plan navigates, fills, waits and extracts once', () => {
  const plan = {
    steps: [
      {
        op: 'goto',
        url: `${live.BASE}/app-search.html`,
        after: { selector: '#q' },
      },
      {
        op: 'act',
        role: 'textbox',
        name: 'Search collection',
        action: 'fill',
        value: 'Ada',
      },
      {
        op: 'act',
        role: 'button',
        name: 'Search',
        action: 'click',
        after: { text: 'Result Ada' },
      },
      { op: 'extract', selector: '#results a' },
      {
        op: 'cdp',
        method: 'Runtime.evaluate',
        params: { expression: 'window.submissions', returnByValue: true },
      },
    ],
  };
  const out = success(
    cli(['run', '--new-tab', 'about:blank', '--json', JSON.stringify(plan)])
  ).out;
  const result = artifact(out, 'browser-result.json');
  if (!result.ok || result.completedSteps !== 5)
    throw new Error('CLI flow did not complete');
  cliTarget = result.steps[0].targetId;
  const rows = JSON.parse(readFileSync(result.steps[3].artifact));
  if (
    rows.length !== 1 ||
    rows[0].text !== 'Result Ada' ||
    !rows[0].href.endsWith('/done?q=Ada')
  )
    throw new Error('Incorrect extraction');
  if (JSON.parse(readFileSync(result.steps[4].artifact)).result.value !== 1)
    throw new Error('Mutation repeated');
});
test('cli: snapshots and installed protocol use selected target', () => {
  // Focused runs do not depend on the previous test.
  if (!cliTarget) {
    const out = success(
      cli([
        'cdp',
        'Target.createTarget',
        '--browser',
        '--params',
        JSON.stringify({ url: `${live.BASE}/app-search.html` }),
      ])
    ).out;
    cliTarget = artifact(out, 'cdp-1.json').targetId;
  }
  expect(
    success(cli(['snapshot', '--target', cliTarget])).out,
    /textbox "Search collection"/
  );
  const out = success(
    cli(['protocol', 'Runtime.evaluate', '--target', cliTarget])
  ).out;
  const schema = artifact(out, 'protocol-1.json');
  if (schema.commands[0]?.name !== 'evaluate')
    throw new Error('Wrong installed protocol member');
  const version = artifact(
    success(cli(['cdp', 'Browser.getVersion', '--browser'])).out,
    'cdp-1.json'
  );
  if (!version.product) throw new Error('Browser CDP command failed');
});
test('cli: ambiguous targets fail before capture', () => {
  for (let i = 0; i < 2; i++)
    success(
      cli([
        'cdp',
        'Target.createTarget',
        '--browser',
        '--params',
        JSON.stringify({ url: `${live.BASE}/app-search.html` }),
      ])
    );
  const refused = cli([
    'snapshot',
    '--target-url',
    `${live.BASE}/app-search.html`,
  ]);
  if (refused.code === 0 || !refused.out.includes('Ambiguous target'))
    throw new Error('Ambiguous target was accepted');
  if (refused.out.includes('[SNAPSHOT]'))
    throw new Error('Captured an arbitrary matching target');
  const targets = success(cli(['targets'])).out;
  if (!targets.includes('/app-search.html'))
    throw new Error('Targets discovery failed');
});
test('cli: cancellation reaches sandbox runner and records interruption', async () => {
  const file = join(live.WORK, 'cli-interrupt.mjs');
  writeFileSync(
    file,
    'export async function run(cdp) { console.log("CLI_INTERRUPT_READY"); await new Promise(resolve => setTimeout(resolve, 30000)); }'
  );
  const proc = spawn(
    process.execPath,
    [
      join(live.ROOT, 'dist/engine/cli.mjs'),
      'script',
      file,
      '--port',
      live.CDP_PORT,
      '--new-tab',
      'about:blank',
    ],
    { cwd: live.WORK, stdio: ['ignore', 'pipe', 'pipe'] }
  );
  let output = '';
  proc.stdout.on('data', chunk => {
    output += chunk;
  });
  proc.stderr.on('data', chunk => {
    output += chunk;
  });
  const exited = new Promise(resolve =>
    proc.once('exit', (code, signal) => resolve({ code, signal }))
  );
  const started = Date.now();
  try {
    while (!output.includes('CLI_INTERRUPT_READY')) {
      if (Date.now() - started > 10000)
        throw new Error('CLI runner did not start: ' + output);
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    proc.kill('SIGTERM');
    const end = await Promise.race([
      exited,
      new Promise(resolve => setTimeout(() => resolve(null), 5000).unref()),
    ]);
    if (!end || end.code === 0)
      throw new Error('CLI cancellation did not terminate');
    const history = sessionMeta('run-history.json');
    if (history.runs.at(-1).status !== 'interrupted')
      throw new Error('CLI runner did not record interruption');
  } finally {
    if (proc.exitCode === null) proc.kill('SIGTERM');
  }
});
