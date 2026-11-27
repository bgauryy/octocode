import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  LOCK_PATH, MANIFEST_PATH, assetNames, buildLock, diffLocks, parseManifest, renderLock, withReleaseTag,
} from './lsp-lock.mjs';

const FIXTURES = path.join(path.dirname(fileURLToPath(import.meta.url)), 'fixtures/lsp-lock');
// Responses of GET /repos/{repo}/releases/tags/{tag}, recorded 2026-11-03 (fields the generator reads).
const recorded = () => ({
  clangd: JSON.parse(readFileSync(path.join(FIXTURES, 'clangd-22.1.0.json'), 'utf8')),
  'rust-analyzer': JSON.parse(readFileSync(path.join(FIXTURES, 'rust-analyzer-2026-06-22.json'), 'utf8')),
});
const manifest = () => parseManifest(readFileSync(MANIFEST_PATH, 'utf8'));
const pinned = servers => Object.fromEntries(servers.map(s => [s.name, s.tag]));

test('the manifest table in managed.rs parses into servers with repo, tag and assets', () => {
  const servers = manifest();
  assert.ok(servers.length >= 2);
  for (const server of servers) {
    assert.match(server.repo, /^[\w.-]+\/[\w.-]+$/);
    assert.ok(server.tag);
    assert.ok(server.assets.every(({ platform, template }) => platform && template));
  }
});

test('each locked asset carries the digest GitHub reported for exactly that asset name', () => {
  const servers = manifest();
  const releases = recorded();
  // The recorded fixtures are for the tags the manifest pins; a bump re-records them.
  assert.deepEqual(pinned(servers), { clangd: releases.clangd.tag_name, 'rust-analyzer': releases['rust-analyzer'].tag_name });
  const lock = buildLock(servers, releases);
  for (const server of servers) {
    const entry = lock.servers[server.name];
    assert.equal(entry.repo, server.repo);
    assert.equal(entry.tag, server.tag);
    assert.equal(entry.immutable, releases[server.name].immutable);
    assert.deepEqual(Object.keys(entry.assets).sort(), assetNames(server).sort());
    for (const [name, asset] of Object.entries(entry.assets)) {
      const upstream = releases[server.name].assets.find(a => a.name === name);
      assert.equal(`sha256:${asset.sha256}`, upstream.digest, name);
      assert.equal(asset.size, upstream.size, name);
      assert.equal(asset.url, upstream.browser_download_url, name);
    }
  }
  // Two platforms that name one asset share one entry: no second copy of a hash.
  const clangd = servers.find(s => s.name === 'clangd');
  assert.ok(assetNames(clangd).length < clangd.assets.length);
});

test('the committed lock is what the generator makes from the recorded releases', () => {
  const committed = readFileSync(LOCK_PATH, 'utf8');
  assert.equal(committed, renderLock(buildLock(manifest(), recorded())));
});

const refusal = (mutate, pattern) => {
  const releases = recorded();
  mutate(releases);
  assert.throws(() => buildLock(manifest(), releases), pattern);
};

test('a missing asset is refused', () => {
  refusal(r => { r.clangd.assets = r.clangd.assets.filter(a => !a.name.startsWith('clangd-mac-')); }, /has no asset clangd-mac-/);
});

test('a null or malformed digest is refused', () => {
  refusal(r => { r['rust-analyzer'].assets[0].digest = null; }, /digest null, not sha256/);
  refusal(r => { r.clangd.assets.find(a => a.name.startsWith('clangd-linux-')).digest = 'sha1:abc'; }, /not sha256/);
});

test('two different assets sharing a digest are refused', () => {
  refusal(r => {
    const [linux, mac] = ['clangd-linux-', 'clangd-mac-'].map(prefix => r.clangd.assets.find(a => a.name.startsWith(prefix)));
    mac.digest = linux.digest;
  }, /shares digest/);
});

test('a release answered for another tag is refused', () => {
  refusal(r => { r.clangd.tag_name = 'other'; }, /answered tag other/);
});

test('check reports every changed value', () => {
  const lock = buildLock(manifest(), recorded());
  const replaced = structuredClone(lock);
  const [name] = Object.keys(replaced.servers.clangd.assets);
  replaced.servers.clangd.assets[name].sha256 = '0'.repeat(64);
  assert.deepEqual(diffLocks(lock, lock), []);
  const changes = diffLocks(lock, replaced);
  assert.equal(changes.length, 1);
  assert.match(changes[0], new RegExp(`servers\\.clangd\\.assets\\.${name.replaceAll('.', '\\.')}\\.sha256`));
});

test('bump rewrites only the named server release tag', () => {
  const source = readFileSync(MANIFEST_PATH, 'utf8');
  const [clangd, other] = manifest();
  const bumped = parseManifest(withReleaseTag(source, clangd, '99.0.0'));
  assert.equal(bumped[0].tag, '99.0.0');
  assert.ok(assetNames(bumped[0]).every(name => !name.includes(clangd.tag)));
  assert.equal(bumped[1].tag, other.tag);
});
