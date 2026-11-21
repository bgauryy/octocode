import test from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { presets, researchSpec, spec } from '../../dist/runtime.js';

const names = surface => surface.commands.map(c => c.name);

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
    argv('open', { attach: true, acknowledge: true, port: 9222 }).args,
    ['--port', '9222', '--attach', '--acknowledge']
  );
});

test('open refuses attach with launch fields, tab-group fields, bad origins and commas', () => {
  const open = schema('open');
  for (const bad of [
    { attach: true },
    { attach: true, port: 9222 },
    { attach: true, acknowledge: true },
    {
      attach: true,
      acknowledge: true,
      port: 9222,
      url: 'https://a.example.com',
    },
    { attach: true, acknowledge: true, port: 9222, headless: true },
    { attach: true, acknowledge: true, port: 9222, noSandbox: true },
    { urls: ['https://a.example.com'] },
    { background: true },
    { newWindow: true },
    { isolated: true },
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
  const flowNames = Object.values(JSON.parse(flow.stdout).areas)
    .flat()
    .map(row => row.name);
  assert.ok(flowNames.includes('login-form'));
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

test('presets nest (slim < default < full); the default holds flow, the full surface alone holds proxy and dev-check; cdp and protocol are in the default', () => {
  const [slim, research, full] = [presets.slim, researchSpec, spec].map(names);
  for (const [small, large] of [
    [slim, research],
    [research, full],
  ]) {
    assert.ok(small.length < large.length);
    for (const name of small) assert.ok(large.includes(name), name);
  }
  assert.ok(research.includes('flow'));
  for (const name of ['proxy', 'dev-check']) {
    assert.ok(!research.includes(name), name);
    assert.ok(full.includes(name), name);
  }
  for (const name of ['cdp', 'protocol']) {
    assert.ok(research.includes(name), name);
    assert.ok(!names(presets.slim).includes(name), name);
  }
});
test('the skill quickstart and shared instructions name only tools the default surface holds', () => {
  const skill = readFileSync(
    new URL('../../skills/octocode-chrome-devtools/SKILL.md', import.meta.url),
    'utf8'
  );
  const firstFlow = skill
    .split('## Quickstart')[1]
    .split('\n## ')[0]
    .split('Run `schema')[0];
  const research = names(researchSpec);
  const named = (text, list) =>
    list.filter(name => new RegExp(`(^|[^\\w-])${name}([^\\w-]|$)`).test(text));
  // snapshot and screenshot are plan steps in the default surface; their tools are full-only.
  for (const name of named(firstFlow, ['open', 'run']))
    assert.ok(research.includes(name), name);
  assert.ok(!research.includes('snapshot') && !research.includes('screenshot'));
  assert.match(firstFlow, /`snapshot` and `screenshot` plan steps/);
  // Hyphenated names cannot be confused with ordinary words (step, check, script).
  const outside = names(spec).filter(
    name => !research.includes(name) && name.includes('-')
  );
  // The default preset's own instructions name no tool outside the default surface.
  assert.deepEqual(named(researchSpec.instructions, outside), []);
});

test('open emulation travels as one JSON flag; geolocation needs acknowledge; reset stands alone', () => {
  const emulation = { colorScheme: 'dark', width: 390, height: 844 };
  const { args } = argv('open', { port: 9222, emulation });
  assert.equal(
    args[args.indexOf('--emulation') + 1],
    JSON.stringify(emulation)
  );
  const open = schema('open');
  const geolocation = { latitude: 48.8, longitude: 2.3 };
  for (const bad of [
    { emulation: { geolocation } },
    { emulation: { reset: true, width: 1 } },
    { emulation: { colorScheme: 'sepia' } },
    { emulation: { bogus: 1 } },
  ])
    assert.equal(open.safeParse(bad).success, false, JSON.stringify(bad));
  assert.equal(
    open.safeParse({ emulation: { geolocation }, acknowledge: true }).success,
    true
  );
});

test('cdp enable and scope map to --enable and --scope and are checked for shape', () => {
  const call = { method: 'Page.enable', connection: { port: 9222 } };
  const { args } = argv('cdp', {
    ...call,
    enable: ['Network', 'Page'],
    scope: 'browser',
  });
  assert.equal(args[args.indexOf('--enable') + 1], 'Network,Page');
  assert.equal(args[args.indexOf('--scope') + 1], 'browser');
  const cdp = schema('cdp');
  for (const bad of [
    { enable: ['network'] },
    { enable: ['A', 'A'] },
    { scope: 'tab' },
  ])
    assert.equal(cdp.safeParse({ ...call, ...bad }).success, false);
});
