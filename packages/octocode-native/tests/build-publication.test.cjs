'use strict';

const assert = require('node:assert/strict');
const { test } = require('node:test');
const { mkdtempSync, mkdirSync, copyFileSync, readFileSync, writeFileSync, rmSync } = require('node:fs');
const { join } = require('node:path');
const { tmpdir } = require('node:os');
const { spawnSync } = require('node:child_process');

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'octocode-release-version-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'scripts'));
  for (const file of ['workspace-metadata.cjs', 'check-version-consistency.cjs', 'sync-versions.cjs']) {
    copyFileSync(join(__dirname, '..', 'scripts', file), join(root, 'scripts', file));
  }
  writeFileSync(join(root, 'Cargo.toml'), '[workspace]\nmembers = ["crates/first", "crates/second"]\nresolver = "2"\n\n[workspace.package]\nversion = "1.0.0"\nedition = "2024"\n');
  for (const name of ['first', 'second']) {
    mkdirSync(join(root, 'crates', name, 'src'), { recursive: true });
    writeFileSync(join(root, 'crates', name, 'Cargo.toml'), `[package]\nname = "release-${name}"\nversion.workspace = true\nedition.workspace = true\npublish = false\n`);
    writeFileSync(join(root, 'crates', name, 'src/lib.rs'), '');
  }
  mkdirSync(join(root, 'npm', 'darwin-arm64'), { recursive: true });
  writeFileSync(join(root, 'package.json'), JSON.stringify({ name: '@test/native', version: '1.0.0', optionalDependencies: { '@test/native-darwin-arm64': '1.0.0' } }));
  writeFileSync(join(root, 'npm/darwin-arm64/package.json'), JSON.stringify({ name: '@test/native-darwin-arm64', version: '1.0.0' }));
  const lock = spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: root, encoding: 'utf8' });
  assert.equal(lock.status, 0, lock.stderr);
  return root;
}

const run = (root, script) => spawnSync(process.execPath, [join(root, 'scripts', script)], { cwd: root, encoding: 'utf8' });
const json = path => JSON.parse(readFileSync(path, 'utf8'));

test('version validation is read-only and rejects a mismatched newly added workspace crate', t => {
  const root = fixture(t);
  const before = readFileSync(join(root, 'Cargo.lock'), 'utf8');
  assert.equal(run(root, 'check-version-consistency.cjs').status, 0);
  assert.equal(readFileSync(join(root, 'Cargo.lock'), 'utf8'), before);
  const manifest = join(root, 'crates/second/Cargo.toml');
  writeFileSync(manifest, readFileSync(manifest, 'utf8').replace('version.workspace = true', 'version = "2.0.0"'));
  const result = run(root, 'check-version-consistency.cjs');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /release-second crate is 2.0.0, expected 1.0.0/);
  assert.equal(readFileSync(join(root, 'Cargo.lock'), 'utf8'), before);
});

test('version sync updates inherited versions and lockfile without a compatibility package', t => {
  const root = fixture(t);
  const packagePath = join(root, 'package.json');
  const pkg = json(packagePath);
  pkg.version = '2.0.0';
  writeFileSync(packagePath, JSON.stringify(pkg));
  const result = run(root, 'sync-versions.cjs');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(json(packagePath).optionalDependencies['@test/native-darwin-arm64'], '2.0.0');
  assert.equal(json(join(root, 'npm/darwin-arm64/package.json')).version, '2.0.0');
  assert.match(readFileSync(join(root, 'Cargo.toml'), 'utf8'), /version = "2.0.0"/);
  assert.doesNotMatch(readFileSync(join(root, 'Cargo.lock'), 'utf8'), /version = "1.0.0"/);
  assert.equal(run(root, 'check-version-consistency.cjs').status, 0);
});

test('version validation rejects mismatched platform pins before publication', t => {
  const root = fixture(t);
  const packagePath = join(root, 'package.json');
  const pkg = json(packagePath);
  pkg.optionalDependencies['@test/native-darwin-arm64'] = '0.9.0';
  writeFileSync(packagePath, JSON.stringify(pkg));
  const result = run(root, 'check-version-consistency.cjs');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /@test\/native-darwin-arm64 is 0.9.0, expected 1.0.0/);
});
