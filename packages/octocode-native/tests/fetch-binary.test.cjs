'use strict';

const assert = require('node:assert/strict');
const { test } = require('node:test');
const { createHash } = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync } = require('node:fs');
const { join } = require('node:path');
const { tmpdir } = require('node:os');
const { extractTarFile, fetchNativeBinary, registryFor } = require('../bin/fetch-binary.cjs');

const SUFFIX = 'linux-x64-gnu';
const VERSION = '1.2.3';
const BINARY = Buffer.from('#!/bin/sh\necho "octocode 1.2.3"\n');

/** A real npm-style tarball: `package/octocode` and `package/package.json`, gzipped. */
function tarball() {
  const root = mkdtempSync(join(tmpdir(), 'octocode-pack-'));
  mkdirSync(join(root, 'package'));
  writeFileSync(join(root, 'package', 'package.json'), '{}');
  writeFileSync(join(root, 'package', 'octocode'), BINARY, { mode: 0o755 });
  const archive = join(root, 'pkg.tgz');
  execFileSync('tar', ['-czf', archive, '-C', root, 'package']);
  return readFileSync(archive);
}

function registry(bytes, { integrity } = {}) {
  const calls = [];
  const sha512 = `sha512-${createHash('sha512').update(bytes).digest('base64')}`;
  const fetch = async url => {
    calls.push(url);
    if (url.endsWith('.tgz')) return { ok: true, arrayBuffer: async () => bytes };
    return {
      ok: true,
      json: async () => ({ dist: { tarball: 'https://cdn.example/pkg.tgz', integrity: integrity ?? sha512 } }),
    };
  };
  return { calls, fetch };
}

const options = (home, fetch, run = () => `octocode ${VERSION}\n`) => ({
  suffix: SUFFIX,
  version: VERSION,
  home,
  env: {},
  fetch,
  log: () => {},
  run,
});

test('registryFor follows the scope registry, then the default registry, like npm', () => {
  const name = '@octocodeai/octocode-native-linux-x64-gnu';
  assert.equal(registryFor(name, {}), 'https://registry.npmjs.org/');
  assert.equal(registryFor(name, { npm_config_registry: 'https://mirror.example/npm' }), 'https://mirror.example/npm/');
  assert.equal(
    registryFor(name, {
      npm_config_registry: 'https://mirror.example/npm/',
      'npm_config_@octocodeai:registry': 'https://scoped.example/',
    }),
    'https://scoped.example/'
  );
});

test('downloads, verifies and caches the platform binary once', async () => {
  const home = mkdtempSync(join(tmpdir(), 'octocode-home-'));
  const { calls, fetch } = registry(tarball());
  const path = await fetchNativeBinary(options(home, fetch));

  assert.equal(path, join(home, 'native', VERSION, SUFFIX, 'octocode'));
  assert.deepEqual(readFileSync(path), BINARY);
  if (process.platform !== 'win32') assert.equal(statSync(path).mode & 0o111, 0o111);
  assert.deepEqual(calls, [
    'https://registry.npmjs.org/@octocodeai%2foctocode-native-linux-x64-gnu/1.2.3',
    'https://cdn.example/pkg.tgz',
  ]);
  assert.deepEqual(readdirSync(join(home, 'native', VERSION, SUFFIX)), ['octocode']);

  assert.equal(await fetchNativeBinary(options(home, fetch)), path);
  assert.equal(calls.length, 2, 'the cached binary needs no network');
});

test('refuses a tarball whose sha512 does not match the registry', async () => {
  const home = mkdtempSync(join(tmpdir(), 'octocode-home-'));
  const { fetch } = registry(tarball(), { integrity: `sha512-${Buffer.alloc(64).toString('base64')}` });
  await assert.rejects(fetchNativeBinary(options(home, fetch)), /integrity mismatch/);
  assert.equal(existsSync(join(home, 'native', VERSION, SUFFIX, 'octocode')), false);
});

test('refuses a binary that reports another version and leaves no file behind', async () => {
  const home = mkdtempSync(join(tmpdir(), 'octocode-home-'));
  const { fetch } = registry(tarball());
  await assert.rejects(
    fetchNativeBinary(options(home, fetch, () => 'octocode 9.9.9')),
    /reports 'octocode 9\.9\.9', expected 'octocode 1\.2\.3'/
  );
  assert.deepEqual(readdirSync(join(home, 'native', VERSION, SUFFIX)), []);
});

test('names the HTTP failure', async () => {
  const home = mkdtempSync(join(tmpdir(), 'octocode-home-'));
  const fetch = async () => ({ ok: false, status: 404 });
  await assert.rejects(fetchNativeBinary(options(home, fetch)), /answered HTTP 404/);
});

test('extractTarFile reads a pax long path and misses an absent file', () => {
  const block = (name, size, type) => {
    const header = Buffer.alloc(512);
    header.write(name, 0);
    header.write(size.toString(8).padStart(11, '0'), 124);
    header.write(type, 156);
    return header;
  };
  const pad = data => Buffer.concat([data, Buffer.alloc((512 - (data.length % 512)) % 512)]);
  const long = `package/${'d/'.repeat(60)}octocode`;
  const record = ` path=${long}\n`;
  const pax = Buffer.from(`${record.length + String(record.length + 3).length}${record}`);
  const body = Buffer.from('binary');
  const tar = Buffer.concat([
    block('PaxHeader', pax.length, 'x'),
    pad(pax),
    block('truncated', body.length, '0'),
    pad(body),
    Buffer.alloc(1024),
  ]);
  assert.deepEqual(extractTarFile(tar, long), body);
  assert.equal(extractTarFile(tar, 'package/octocode'), null);
});
