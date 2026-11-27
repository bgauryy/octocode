import test from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { presets, spec } from '../../dist/runtime.js';
import {
  chromeCommands,
  chromeHelperNames,
} from '../../dist/engine/chrome-contract.mjs';

const directory = mkdtempSync(join(tmpdir(), 'chrome-typed-group-helper-'));
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
const run = (file, args) =>
  spawnSync(process.execPath, [engine(file), ...args], { encoding: 'utf8' });
const names = surface => surface.commands.map(c => c.name);

test('the helper gateway replaces the nine args-only tools and only the full preset carries it', () => {
  assert.ok(chromeHelperNames.length > 0);
  for (const name of chromeHelperNames) {
    assert.ok(!names(spec).includes(name), name);
    assert.ok(!Object.hasOwn(chromeCommands, name), name);
    assert.equal(
      spec.commands
        .find(command => command.name === 'helper')
        .schema.safeParse({ name }).success,
      true,
      name
    );
  }
  assert.ok(names(presets.full).includes('helper'));
  assert.ok(!names(presets.research).includes('helper'));
  assert.ok(!names(presets.slim).includes('helper'));
});

test('helper runs the named engine command with its own arguments', () => {
  for (const name of chromeHelperNames)
    assert.deepEqual(argv('helper', { name, args: ['--help'] }), {
      command: name,
      args: ['--help'],
      stdin: undefined,
    });
  assert.deepEqual(argv('helper', { name: 'prune' }).args, []);
  for (const bad of [{}, { name: 'cookies' }, { name: 'prune', args: [1] }])
    assert.equal(schema('helper').safeParse(bad).success, false);
});

test('group maps to its action then name, urls, flags and the port', () => {
  assert.deepEqual(
    argv('group', {
      action: 'open',
      name: 'g1',
      urls: ['https://a.example.com', 'about:blank'],
      background: true,
      newWindow: true,
      port: 9222,
    }),
    {
      command: 'group',
      args: [
        'open',
        '--name',
        'g1',
        '--urls',
        'https://a.example.com,about:blank',
        '--background',
        '--new-window',
        '--port',
        '9222',
      ],
      stdin: undefined,
    }
  );
  assert.deepEqual(argv('group', { action: 'open', port: 9222 }).args, [
    'open',
    '--port',
    '9222',
  ]);
  assert.deepEqual(
    argv('group', { action: 'open', private: false, port: 9222 }).args,
    ['open', '--no-private', '--port', '9222']
  );
  assert.deepEqual(
    argv('group', { action: 'close', name: 'g1', tab: 'T1', port: 9222 }).args,
    ['close', '--name', 'g1', '--tab', 'T1', '--port', '9222']
  );
  assert.deepEqual(argv('group', { action: 'list', port: 9222 }).args, [
    'list',
    '--port',
    '9222',
  ]);
});

test('the mapped group argv passes the engine parser and reaches the group engine', () => {
  const { args } = argv('group', {
    action: 'add',
    name: 'g1',
    urls: ['https://a.example.com'],
    port: 9222,
  });
  const result = run('group-command.mjs', args);
  assert.equal(result.status, 2);
  // Past the parser, the engine refuses an unknown group by name before any browser work.
  assert.match(result.stderr, /\[GROUP\] Unknown group "g1"/);
  assert.match(run('group-command.mjs', ['--help']).stdout, /group open\|add/);
  // The engine parser refuses what the typed schema never produces.
  for (const bad of [
    ['open'],
    ['open', '--port', '9222', '--tab', 'T'],
    ['add', '--port', '9222', '--name', 'g'],
    ['add', '--port', '9222', '--urls', 'about:blank'],
    ['close', '--port', '9222'],
    ['list', '--port', '9222', '--name', 'g'],
    ['open', '--port', '9222', '--private', '--no-private'],
    ['open', '--port', '9222', '--urls', 'ftp://x'],
    ['rename', '--port', '9222'],
  ]) {
    const refused = run('group-command.mjs', bad);
    assert.equal(refused.status, 2, bad.join(' '));
    assert.doesNotMatch(refused.stderr, /not implemented/, bad.join(' '));
  }
});

test('the engine CLI reaches a helper by name and through helper <name>', () => {
  for (const args of [
    ['prune', '--help'],
    ['helper', 'prune', '--help'],
  ]) {
    const result = run('cli.mjs', args);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout + result.stderr, /max-age-days/);
  }
});
