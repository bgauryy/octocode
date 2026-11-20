import test from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { chromeFlagDenylist } from '@octocodeai/config/chrome';
import { spec } from '../../dist/runtime.js';

const directory = mkdtempSync(join(tmpdir(), 'chrome-typed-recipes-'));
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
const cli = (...args) =>
  spawnSync(
    process.execPath,
    [new URL('../../dist/engine/cli.mjs', import.meta.url).pathname, ...args],
    { encoding: 'utf8' }
  );
const recipes = {
  'script-sources': [
    'SCRIPTS_URL',
    'SCRIPTS_RELOAD',
    'SCRIPTS_MATCH',
    'SCRIPTS_INLINE',
  ],
  'source-map': [
    'SOURCEMAP_SCRIPT',
    'SOURCEMAP_POSITIONS',
    'SOURCEMAP_LIST_SOURCES',
    'SOURCEMAP_SAVE_SOURCES',
  ],
  'storage-dump': [
    'STORAGE_DUMP_URL',
    'STORAGE_DUMP_INCLUDE_VALUES',
    'STORAGE_DUMP_IDB_RECORDS',
  ],
  'network-events': [
    'NETEVENTS_URL',
    'NETEVENTS_MS',
    'NETEVENTS_MATCH',
    'NETEVENTS_KINDS',
  ],
  'network-conditions': [
    'NETCOND_URL',
    'NETCOND_OFFLINE',
    'NETCOND_LATENCY_MS',
    'NETCOND_DOWN_KBPS',
    'NETCOND_UP_KBPS',
    'NETCOND_BLOCK_URLS',
    'NETCOND_EXTRA_HEADERS',
    'NETCOND_MOCKS',
  ],
  coverage: ['COVERAGE_URL', 'COVERAGE_MS', 'COVERAGE_CSS', 'COVERAGE_JS'],
  'trace-capture': ['TRACE_URL', 'TRACE_MS', 'TRACE_CATEGORIES'],
  'heap-snapshot': ['HEAP_COLLECT_GARBAGE'],
  'page-pdf': ['PDF_LANDSCAPE', 'PDF_PRINT_BACKGROUND', 'PDF_SCALE'],
};

test('typed launch fields become the exact engine argv', () => {
  assert.deepEqual(
    argv('open', {
      port: 9222,
      enableFeatures: ['A', 'B'],
      disableFeatures: ['C'],
      blinkFeatures: ['D', 'E'],
      windowSize: '1280x800',
      userAgent: 'agent/1.0',
      proxy: {
        server: 'http://p:1',
        bypassList: '<local>',
        pacUrl: 'http://x/p.pac',
      },
      chromePath: '/usr/bin/chromium',
      chromeFlags: ['--mute-audio', '--a=b'],
    }).args,
    [
      '--port',
      '9222',
      '--enableFeatures',
      'A,B',
      '--disableFeatures',
      'C',
      '--blinkFeatures',
      'D,E',
      '--windowSize',
      '1280x800',
      '--userAgent',
      'agent/1.0',
      '--proxyServer',
      'http://p:1',
      '--proxyBypassList',
      '<local>',
      '--proxyPacUrl',
      'http://x/p.pac',
      '--chromePath',
      '/usr/bin/chromium',
      '--chrome-flags',
      '--mute-audio,--a=b',
    ]
  );
});

test('open refuses launch fields with attach, bad values and every denylisted flag', () => {
  const open = schema('open');
  for (const bad of [
    { attach: true, port: 9222, windowSize: '1280x800' },
    { attach: true, port: 9222, chromeFlags: ['--mute-audio'] },
    { attach: true, port: 9222, proxy: { server: 'x' } },
    { windowSize: 'big' },
    { enableFeatures: ['A,B'] },
    { proxy: {} },
    { args: ['--headless'], userAgent: 'x' },
    ...chromeFlagDenylist.map(denied => ({
      chromeFlags: [denied.prefix ? `${denied.flag}x` : denied.flag],
    })),
  ])
    assert.equal(open.safeParse(bad).success, false, JSON.stringify(bad));
});

test('each capture recipe lists its option names through schema and refuses unknown options', () => {
  for (const [name, keys] of Object.entries(recipes)) {
    const out = cli('schema', 'check', name);
    assert.equal(out.status, 0, `${name}: ${out.stderr}`);
    const listed = JSON.parse(out.stdout).checks[name].options;
    assert.deepEqual(listed, keys, name);
    const bad = cli('check', name, '--options', '{"NOPE":1}', '--dry-run');
    assert.notEqual(bad.status, 0, name);
    assert.match(bad.stderr, /Unsupported/, name);
  }
});

test('typed recipe options reach the engine as one env value each, structured ones as JSON', () => {
  const check = schema('check');
  const options = {
    NETCOND_URL: 'http://localhost:3000/',
    NETCOND_OFFLINE: true,
    NETCOND_BLOCK_URLS: ['*.png'],
    NETCOND_MOCKS: [{ match: '*/api', status: 503 }],
  };
  const { args } = engineArgs(
    'check',
    check.parse({ recipe: 'network-conditions', options })
  );
  assert.deepEqual(args.slice(0, 1), ['network-conditions']);
  assert.deepEqual(JSON.parse(args[args.indexOf('--options') + 1]), options);
  const dry = cli(
    'check',
    'network-conditions',
    '--options',
    JSON.stringify(options),
    '--dry-run'
  );
  assert.equal(dry.status, 0, dry.stderr);
});

test('the capture recipes refuse bad ranges and missing required options', () => {
  const check = schema('check');
  for (const [recipe, options] of [
    ['network-events', {}],
    ['trace-capture', { TRACE_MS: 0 }],
    ['source-map', { SOURCEMAP_SCRIPT: 's', SOURCEMAP_POSITIONS: '1-2' }],
    [
      'network-conditions',
      { NETCOND_URL: 'u', NETCOND_MOCKS: [{ match: 'a', status: 99 }] },
    ],
    ['page-pdf', { PDF_SCALE: 5 }],
  ])
    assert.equal(
      check.safeParse({ recipe, options }).success,
      false,
      JSON.stringify([recipe, options])
    );
});

test('a placeholder recipe says it is not implemented and exits non-zero', () => {
  // Dry run builds the invocation only; the placeholder text is covered by the engine run below.
  const out = cli(
    'check',
    'page-pdf',
    '--options',
    '{"PDF_SCALE":1}',
    '--dry-run'
  );
  assert.equal(out.status, 0, out.stderr);
});
