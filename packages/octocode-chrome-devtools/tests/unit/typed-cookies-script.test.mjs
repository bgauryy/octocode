import test from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spec } from '../../dist/runtime.js';

const directory = mkdtempSync(join(tmpdir(), 'chrome-typed-args-'));
const outfile = join(directory, 'invocation.mjs');
await build({
  entryPoints: [new URL('../../src/invocation.ts', import.meta.url).pathname],
  outfile,
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node24',
});
const { cookieArgs } = await import(pathToFileURL(outfile).href);
test.after(() => rmSync(directory, { recursive: true, force: true }));

const schema = name => spec.commands.find(c => c.name === name).schema;
const typed = {
  from: { port: 9333 },
  to: { port: 9222 },
  confirmDomains: ['a.example.com', 'b.example.com'],
  urls: ['https://a.example.com'],
  exportStorageState: 'out.json',
  dryRun: true,
  acknowledgeSecrets: true,
};

test('typed cookies input becomes the exact engine argv', () => {
  assert.deepEqual(cookieArgs(typed), [
    '--i-understand-secrets',
    '--from-port',
    '9333',
    '--to-port',
    '9222',
    '--confirm-domains',
    'a.example.com,b.example.com',
    '--urls',
    'https://a.example.com',
    '--export-storage-state',
    'out.json',
    '--dry-run',
  ]);
  assert.deepEqual(
    cookieArgs({
      from: { profile: 'Default', sourcePort: 9444, chromePath: '/c/chrome' },
      to: { port: 9222 },
      confirmDomains: ['a.example.com'],
      acknowledgeSecrets: true,
    }),
    [
      '--i-understand-secrets',
      '--from-profile',
      'Default',
      '--source-port',
      '9444',
      '--to-port',
      '9222',
      '--confirm-domains',
      'a.example.com',
      '--chromePath',
      '/c/chrome',
    ]
  );
  assert.deepEqual(
    cookieArgs({
      from: { storageState: 's.json' },
      to: { port: 9222 },
      acknowledgeSecrets: true,
    }),
    [
      '--i-understand-secrets',
      '--from-storage-state',
      's.json',
      '--to-port',
      '9222',
    ]
  );
});

test('cookies rejects mixing typed fields with args, a missing acknowledgement and missing confirmDomains', () => {
  const cookies = schema('cookies');
  assert.equal(cookies.safeParse(typed).success, true);
  assert.equal(
    cookies.safeParse({ ...typed, args: ['--dry-run'] }).success,
    false
  );
  for (const bad of [
    { acknowledgeSecrets: undefined },
    { acknowledgeSecrets: false },
    { confirmDomains: undefined },
    { confirmDomains: [] },
  ])
    assert.equal(cookies.safeParse({ ...typed, ...bad }).success, false);
});

test('script accepts a typed file with connection and refuses mixing with args', () => {
  const script = schema('script');
  assert.equal(
    script.safeParse({
      file: 'probe.mjs',
      connection: { port: 9222, scriptTimeout: 5000 },
    }).success,
    true
  );
  assert.equal(
    script.safeParse({ file: 'probe.mjs', args: ['probe.mjs'] }).success,
    false
  );
  assert.equal(script.safeParse({}).success, false);
});
