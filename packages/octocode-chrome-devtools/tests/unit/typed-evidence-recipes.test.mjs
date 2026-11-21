import test from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spec } from '../../dist/runtime.js';

const directory = mkdtempSync(join(tmpdir(), 'chrome-evidence-recipes-'));
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
const check = (recipe, options) =>
  schema('check').safeParse({ recipe, options });
const forwarded = (recipe, options) => {
  const { args } = engineArgs(
    'check',
    schema('check').parse({ recipe, options })
  );
  assert.equal(args[0], recipe);
  return JSON.parse(args[args.indexOf('--options') + 1]);
};
const cli = (...args) =>
  spawnSync(
    process.execPath,
    [new URL('../../dist/engine/cli.mjs', import.meta.url).pathname, ...args],
    { encoding: 'utf8' }
  );

const valid = {
  'page-html': {
    PAGEHTML_URL: 'http://localhost/',
    PAGEHTML_SELECTOR: 'main',
    PAGEHTML_FRAMES: true,
    PAGEHTML_SHADOW: '1',
  },
  'page-archive': { ARCHIVE_URL: 'http://localhost/' },
  'page-resources': {
    RES_URL: 'http://localhost/',
    RES_TYPES: ['script', 'xhr'],
    RES_BINARY: false,
    RES_MATCH: '/api',
  },
  'console-log': {
    CONSOLE_URL: 'http://localhost/',
    CONSOLE_MS: 500,
    CONSOLE_LEVELS: ['error', 'warning'],
  },
  downloads: {
    DL_URL: 'http://localhost/',
    DL_CLICK_SELECTOR: 'a.download',
    DL_MS: 3000,
    DL_ACKNOWLEDGE: true,
  },
};

test('each new recipe forwards every option to the engine unchanged', () => {
  for (const [recipe, options] of Object.entries(valid))
    assert.deepEqual(forwarded(recipe, options), options, recipe);
});

test('required options, literals and enums are refused', () => {
  for (const [recipe, options] of [
    ['console-log', {}],
    ['console-log', { CONSOLE_MS: 0 }],
    ['console-log', { CONSOLE_MS: 5, CONSOLE_LEVELS: ['trace'] }],
    ['downloads', { DL_URL: 'u', DL_MS: 5 }],
    ['downloads', { DL_URL: 'u', DL_MS: 5, DL_ACKNOWLEDGE: false }],
    ['downloads', { DL_MS: 5, DL_ACKNOWLEDGE: true }],
    ['downloads', { DL_URL: 'u', DL_ACKNOWLEDGE: true }],
    ['page-resources', { RES_TYPES: ['video'] }],
    ['page-resources', { RES_TYPES: [] }],
    ['page-html', { PAGEHTML_FRAMES: 'yes' }],
    ['page-html', { SCRIPTS_URL: 'u' }],
    ['page-archive', { RES_URL: 'u' }],
  ])
    assert.equal(
      check(recipe, options).success,
      false,
      JSON.stringify([recipe, options])
    );
});

test('schema --command check --recipe returns a field guide', () => {
  const out = cli('schema', 'check', 'page-resources');
  assert.equal(out.status, 0, out.stderr);
  const guide = JSON.parse(out.stdout);
  assert.deepEqual(guide.checks['page-resources'].options, [
    'RES_URL',
    'RES_TYPES',
    'RES_BINARY',
    'RES_MATCH',
  ]);
  const tool = schema('schema').safeParse({
    command: 'check',
    recipe: 'page-resources',
  });
  assert.equal(tool.success, true);
});

test('step save accepts plain names and refuses paths, odd names and repeats', () => {
  const run = steps => schema('run').safeParse({ plan: { steps } });
  const save = name => ({ op: 'evaluate', expression: '1', save: name });
  assert.equal(run([save('page.html')]).success, true);
  assert.equal(
    run([
      save('a.json'),
      { op: 'extract', selector: 'a', save: 'links.json' },
      { op: 'cdp', method: 'DOM.getDocument', save: 'doc-1' },
    ]).success,
    true
  );
  for (const bad of [
    '',
    '.hidden',
    '-x',
    'a/b',
    '..',
    'a b',
    '../x',
    'a'.repeat(82),
  ])
    assert.equal(run([save(bad)]).success, false, bad);
  assert.equal(run([save('a'), save('a')]).success, false);
  assert.equal(
    run([{ op: 'goto', url: 'http://localhost/', save: 'x' }]).success,
    false
  );
});

test('the engine placeholder recipes parse options and report not implemented', () => {
  const dry = cli(
    'check',
    'downloads',
    '--options',
    JSON.stringify(valid.downloads),
    '--dry-run'
  );
  assert.equal(dry.status, 0, dry.stderr);
});
