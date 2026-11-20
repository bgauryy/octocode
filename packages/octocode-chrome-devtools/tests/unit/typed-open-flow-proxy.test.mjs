import test from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { researchSpec, spec } from '../../dist/runtime.js';

const directory = mkdtempSync(join(tmpdir(), 'chrome-typed-open-flow-'));
const outfile = join(directory, 'invocation.mjs');
await build({
  entryPoints: [new URL('../../src/invocation.ts', import.meta.url).pathname],
  outfile,
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node24',
});
const { engineArgs } = await import(pathToFileURL(outfile).href);
test.after(() => rmSync(directory, { recursive: true, force: true }));

const schema = name => spec.commands.find(c => c.name === name).schema;
const argv = (command, input) =>
  engineArgs(command, schema(command).parse(input));
const engine = file =>
  new URL(`../../dist/engine/${file}`, import.meta.url).pathname;
const place = (file, args, input) =>
  spawnSync(process.execPath, [engine(file), ...args], {
    encoding: 'utf8',
    input,
  });
const plan = { steps: [{ op: 'snapshot' }] };

test('typed open fields become the exact engine argv', () => {
  assert.deepEqual(
    argv('open', {
      port: 9222,
      headless: true,
      allowOrigins: ['https://a.example.com', 'http://localhost:5173'],
      noSandbox: true,
    }).args,
    [
      '--port',
      '9222',
      '--headless',
      '--allow-origin',
      'https://a.example.com,http://localhost:5173',
      '--no-sandbox',
    ]
  );
  assert.deepEqual(
    argv('open', {
      urls: ['https://a.example.com', 'about:blank'],
      background: true,
      newWindow: true,
      isolated: true,
    }).args,
    [
      '--urls',
      'https://a.example.com,about:blank',
      '--background',
      '--new-window',
      '--isolated',
    ]
  );
  assert.deepEqual(argv('open', { attach: true, port: 9222 }).args, [
    '--port',
    '9222',
    '--attach',
  ]);
});

test('open refuses attach with launch fields, urls with url, bad origins and commas', () => {
  const open = schema('open');
  for (const bad of [
    { attach: true },
    { attach: true, port: 9222, url: 'https://a.example.com' },
    { attach: true, port: 9222, urls: ['https://a.example.com'] },
    { attach: true, port: 9222, headless: true },
    { attach: true, port: 9222, noSandbox: true },
    { url: 'https://a.example.com', urls: ['https://b.example.com'] },
    { urls: ['https://a.example.com,https://b.example.com'] },
    { urls: ['file:///etc/passwd'] },
    { background: true },
    { allowOrigins: ['https://a.example.com/path'] },
    { allowOrigins: ['https://a.example.com,https://b.example.com'] },
  ])
    assert.equal(open.safeParse(bad).success, false, JSON.stringify(bad));
});

test('connection group maps to --group and --group-index and refuses target selectors', () => {
  assert.deepEqual(
    argv('run', { plan, connection: { port: 9222, group: 'g', groupIndex: 2 } })
      .args,
    ['--port', '9222', '--group', 'g', '--group-index', '2', '--plan', '-']
  );
  const run = schema('run');
  for (const connection of [
    { group: 'g', target: 't' },
    { group: 'g', targetUrl: 'x' },
    { group: 'g', newTab: 'https://a.example.com' },
    { group: 'g', groupIndex: -1 },
    { groupIndex: 1 },
  ])
    assert.equal(
      run.safeParse({ plan, connection }).success,
      false,
      JSON.stringify(connection)
    );
});

test('proxy maps to action, --port, --listen-port and --log', () => {
  assert.deepEqual(
    argv('proxy', { action: 'start', port: 9222, listenPort: 9333, log: true })
      .args,
    ['start', '--port', '9222', '--listen-port', '9333', '--log']
  );
  assert.deepEqual(argv('proxy', { action: 'stop', port: 9222 }).args, [
    'stop',
    '--port',
    '9222',
  ]);
  for (const bad of [
    { action: 'start' },
    { action: 'stop', port: 9222, log: true },
    { action: 'pause', port: 9222 },
  ])
    assert.equal(schema('proxy').safeParse(bad).success, false);
});

test('flow maps to action, name, --params and a stdin plan', () => {
  assert.deepEqual(argv('flow', { action: 'list' }).args, ['list']);
  const run = argv('flow', {
    action: 'run',
    name: 'login-form',
    params: { user: 'a', password: 'hunter2-secret', retries: 2 },
    connection: { port: 9222, group: 'g' },
  });
  // Params ride on stdin: no value, secret or not, may sit in the process argument list.
  assert.deepEqual(run.args, [
    'run',
    'login-form',
    '--params',
    '-',
    '--port',
    '9222',
    '--group',
    'g',
  ]);
  assert.doesNotMatch(run.args.join(' '), /hunter2-secret/);
  assert.deepEqual(JSON.parse(run.stdin), {
    user: 'a',
    password: 'hunter2-secret',
    retries: 2,
  });
  const cdp = argv('cdp', {
    method: 'Network.setCookie',
    params: { name: 'sid', value: 'cookie-secret' },
  });
  assert.doesNotMatch(cdp.args.join(' '), /cookie-secret/);
  assert.equal(JSON.parse(cdp.stdin).value, 'cookie-secret');
  const save = argv('flow', { action: 'save', name: 'mine', plan });
  assert.deepEqual(save.args, ['save', 'mine', '--plan', '-']);
  assert.equal(save.stdin, JSON.stringify(plan));
  assert.equal(argv('flow', { action: 'run', name: 'x' }).stdin, undefined);
  for (const bad of [
    { action: 'show' },
    { action: 'list', name: 'x' },
    { action: 'save', name: 'mine' },
    { action: 'run', name: 'x', plan },
    { action: 'run', name: '../x' },
  ])
    assert.equal(
      schema('flow').safeParse(bad).success,
      false,
      JSON.stringify(bad)
    );
});

test('the mapped argv reaches the real proxy and flow engines', () => {
  const proxy = place(
    'cdp-proxy.mjs',
    argv('proxy', { action: 'status', port: 9 }).args
  );
  assert.equal(proxy.status, 0, proxy.stderr);
  assert.equal(JSON.parse(proxy.stdout).status, 'PROXY_NOT_RUNNING');
  const flow = place('flow-command.mjs', argv('flow', { action: 'list' }).args);
  assert.equal(flow.status, 0, flow.stderr);
  const names = JSON.parse(flow.stdout).flows.map(row => row.name);
  assert.ok(names.includes('login-form'));
});

test('the engines refuse argv the typed schema never produces', () => {
  for (const { file, args } of [
    { file: 'cdp-proxy.mjs', args: ['start'] },
    { file: 'cdp-proxy.mjs', args: ['stop', '--port', '9222', '--log'] },
    { file: 'cdp-proxy.mjs', args: ['start', '--port', '70000'] },
    { file: 'flow-command.mjs', args: ['list', 'x'] },
    { file: 'flow-command.mjs', args: ['run'] },
    { file: 'flow-command.mjs', args: ['run', 'x', '--plan', '-'] },
    {
      file: 'flow-command.mjs',
      args: ['run', 'x', '--group', 'g', '--target', 't'],
    },
    { file: 'flow-command.mjs', args: ['run', 'x', '--group-index', '1'] },
    { file: 'flow-command.mjs', args: ['run', 'x', '--params', '{"a":{}}'] },
  ]) {
    const result = place(file, args);
    const label = [file, ...args].join(' ');
    assert.equal(result.status, 2, label);
    assert.doesNotMatch(result.stderr, /not implemented yet/, label);
  }
});

test('flow is on the default surface and proxy only on the full one', () => {
  const names = surface => surface.commands.map(c => c.name);
  assert.ok(names(researchSpec).includes('flow'));
  assert.ok(!names(researchSpec).includes('proxy'));
  assert.ok(names(spec).includes('proxy'));
});
