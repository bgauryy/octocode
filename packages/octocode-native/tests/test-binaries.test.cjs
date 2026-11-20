'use strict';

// test-binaries.cjs: the test executables named in cargo's JSON messages, their order, and
// libtest's result lines.

const assert = require('node:assert/strict');
const { mkdtempSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');
const { test } = require('node:test');
const {
  countResults,
  crateNamed,
  packageOf,
  slowestFirst,
  testExecutables,
} = require('../scripts/test-binaries.cjs');

test('a package id names its crate and version in every form cargo writes', () => {
  assert.deepEqual(
    packageOf(
      'path+file:///w/crates/tool-github#octocode-tool-github@20.2.0',
      '/w/crates/tool-github/Cargo.toml'
    ),
    { name: 'octocode-tool-github', version: '20.2.0' }
  );
  assert.deepEqual(
    packageOf(
      'path+file:///w/crates/octocode-io#20.2.0',
      '/w/crates/octocode-io/Cargo.toml'
    ),
    { name: 'octocode-io', version: '20.2.0' }
  );
  assert.deepEqual(
    packageOf(
      'octocode-io 20.2.0 (path+file:///w/crates/io)',
      '/w/crates/io/Cargo.toml'
    ),
    { name: 'octocode-io', version: '20.2.0' }
  );
});

test('only test executables are kept, once each, from their package directory', () => {
  const artifact = (kind, isTest, executable) => ({
    reason: 'compiler-artifact',
    package_id: 'path+file:///w/crates/io#octocode-io@1.0.0',
    manifest_path: '/w/crates/io/Cargo.toml',
    target: { kind: [kind], name: 'octocode_io' },
    profile: { test: isTest },
    executable,
  });
  const found = testExecutables([
    artifact('lib', true, '/t/deps/octocode_io-1'),
    artifact('lib', true, '/t/deps/octocode_io-1'),
    artifact('bin', false, '/t/debug/octocode'),
    artifact('lib', true, null),
    { reason: 'build-finished', success: true },
  ]);
  assert.deepEqual(found, [
    {
      crate: 'octocode-io',
      version: '1.0.0',
      kind: 'lib',
      target: 'octocode_io',
      executable: '/t/deps/octocode_io-1',
      dir: '/w/crates/io',
    },
  ]);
});

test('a crate is found with or without its octocode- prefix', () => {
  const crates = new Set(['octocode-tool-github', 'octocode-engine']);
  assert.equal(crateNamed('tool-github', crates), 'octocode-tool-github');
  assert.equal(crateNamed('octocode-engine', crates), 'octocode-engine');
  assert.equal(crateNamed('graph', crates), null);
});

test('the slowest binary starts first: last seconds when known, then size', t => {
  const dir = mkdtempSync(path.join(tmpdir(), 'octocode-test-binaries-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const binary = (target, bytes) => {
    const executable = path.join(dir, target);
    writeFileSync(executable, Buffer.alloc(bytes));
    return { crate: 'c', kind: 'lib', target, executable };
  };
  const [small, big, timed] = [
    binary('small', 10),
    binary('big', 1000),
    binary('timed', 1),
  ];
  assert.deepEqual(
    slowestFirst([small, big, timed], { 'c/lib/timed': 90 }).map(b => b.target),
    ['timed', 'big', 'small']
  );
});

test('result lines are summed across a binary output', () => {
  const text = [
    'test a ... ok',
    'test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.10s',
    'test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s',
  ].join('\n');
  assert.deepEqual(countResults(text), { passed: 5, failed: 1, ignored: 1 });
});
