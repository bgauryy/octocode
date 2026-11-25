import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkVersions, parseArgs, parseSpec } from '../check-versions.mjs';

// A fake npm registry: package → abbreviated metadata.
const registry = {
  typescript: { 'dist-tags': { latest: '7.0.2', next: '7.1.0-dev.1' }, versions: { '6.0.2': {}, '6.0.3': {}, '7.0.2': {}, '7.1.0-dev.1': {} } },
  'typescript-eslint': { 'dist-tags': { latest: '8.74.0' }, versions: {
    '8.17.0': { peerDependencies: { eslint: '^9.0.0' }, peerDependenciesMeta: { typescript: { optional: true } } },
    '8.74.0': { peerDependencies: { eslint: '^9.0.0 || ^10.0.0', typescript: '>=4.8.4 <6.1.0' } },
  } },
  eslint: { 'dist-tags': { latest: '10.15.0' }, versions: { '10.15.0': { engines: { node: '^20.19.0 || ^22.13.0 || >=24' } } } },
  vitest: { 'dist-tags': { latest: '5.1.2' }, versions: { '5.1.2': { peerDependencies: { vite: '^8.0.0', jsdom: '*' }, peerDependenciesMeta: { jsdom: { optional: true } } } } },
  request: { 'dist-tags': { latest: '2.88.2' }, versions: { '2.88.2': { deprecated: 'request has been deprecated' } } },
  '@types/node': { 'dist-tags': { latest: '26.6.10' }, versions: { '20.19.43': {}, '26.6.10': {} } },
  vite: { 'dist-tags': { latest: '8.3.8' }, versions: { '8.3.8': {} } },
};
registry.vitest.versions['5.1.2'].peerDependencies['@types/node'] = '^22.0.0 || >=24.0.0';
registry.vitest.versions['5.1.2'].peerDependenciesMeta['@types/node'] = { optional: true };

const fakeFetch = async url => {
  const name = decodeURIComponent(new URL(url).pathname.slice(1));
  const body = registry[name];
  return { ok: Boolean(body), status: body ? 200 : 404, json: async () => body };
};

const run = (specs, node = '26.13.1') => checkVersions({ specs, node, registry: 'https://registry.test' }, fakeFetch);

test('a chosen peer outside the declared range is a conflict with fixes that declare support', async () => {
  const report = await run(['typescript', 'typescript-eslint', 'eslint']);
  assert.equal(report.ok, false);
  assert.deepEqual(report.conflicts, [{ package: 'typescript-eslint', version: '8.74.0', peer: 'typescript', range: '>=4.8.4 <6.1.0',
    chosen: '7.0.2', fixPeer: '6.0.3', fixPackage: null }]);
});

test('a range or dist-tag picks the version, and a fitting set is ok', async () => {
  const report = await run(['typescript@~6.0', 'typescript-eslint', 'eslint']);
  assert.equal(report.ok, true);
  assert.deepEqual(report.packages.map(entry => `${entry.name}@${entry.version}`), ['typescript@6.0.3', 'typescript-eslint@8.74.0', 'eslint@10.15.0']);
  assert.equal((await run(['typescript@next'])).packages[0].version, '7.1.0-dev.1');
});

test('required peers outside the set are listed; optional peers are not', async () => {
  const report = await run(['vitest']);
  assert.deepEqual(report.missingPeers, [{ package: 'vitest', peer: 'vite', range: '^8.0.0' }]);
  assert.equal(report.ok, true, 'a missing peer is advice, not a failure');
});

test('deprecations, engine misfits and unknown packages fail the check', async () => {
  const report = await run(['request', 'eslint', 'no-such-package'], '21.0.0');
  assert.equal(report.ok, false);
  assert.equal(report.deprecated[0].name, 'request');
  assert.deepEqual(report.engineMismatches.map(entry => entry.name), ['eslint']);
  assert.deepEqual(report.errors.map(entry => entry.code), ['not-found']);
});

test('a range nothing matches is a no-match error', async () => {
  assert.deepEqual((await run(['typescript@>=9'])).errors.map(entry => entry.code), ['no-match']);
});

test('specs and options parse; scoped names keep their @', () => {
  assert.deepEqual(parseSpec('@types/node@^26'), { ecosystem: 'npm', name: '@types/node', requested: '^26' });
  assert.deepEqual(parseSpec('crates:serde'), { ecosystem: 'crates', name: 'serde', requested: 'latest' });
  assert.deepEqual(parseSpec('pypi:django>=5.2,<6'), { ecosystem: 'pypi', name: 'django', requested: '>=5.2,<6' });
  assert.deepEqual(parseSpec('maven:org.acme:core@[1.0,2.0)'), { ecosystem: 'maven', name: 'org.acme:core', requested: '[1.0,2.0)' });
  assert.deepEqual(parseSpec('jsr:@std/path@^1'), { ecosystem: 'jsr', name: '@std/path', requested: '^1' });
  assert.deepEqual(parseSpec('rubygems:rails@~> 8.1'), { ecosystem: 'rubygems', name: 'rails', requested: '~> 8.1' });
  assert.equal(parseArgs(['vite', '--node', 'v26.13.1']).node, '26.13.1');
  assert.throws(() => parseArgs([]), /at least one/);
  assert.throws(() => parseArgs(['vite', '--node', '26']), /full version/);
});

test('--project adds the declared ranges, so a scaffold pin that breaks a new peer is a conflict', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'octocode-generate-versions-'));
  try {
    writeFileSync(join(dir, 'package.json'), JSON.stringify({ devDependencies: { '@types/node': '^20', local: 'workspace:*', vite: '^8' } }));
    const report = await checkVersions({ specs: ['vitest'], node: '26.13.1', registry: 'https://registry.test', project: dir }, fakeFetch);
    assert.deepEqual(report.conflicts.map(entry => [entry.package, entry.peer, entry.chosen, entry.fixPeer]), [['vitest', '@types/node', '20.19.43', '26.6.10']]);
    assert.deepEqual(report.install, ['vitest@5.1.2'], 'install lists exact versions for the command-line specs only');
    assert.ok(report.packages.find(entry => entry.name === '@types/node').declared);
    assert.deepEqual(report.unchecked, [{ where: 'local in package.json', range: 'workspace:*' }]);
    const replaced = await checkVersions({ specs: ['vitest', '@types/node@^26'], node: '26.13.1', registry: 'https://registry.test', project: dir }, fakeFetch);
    assert.equal(replaced.conflicts.length, 0, 'a command-line spec replaces the declared range');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
