import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const WORKSPACE_HEALTH_LIB = ['lib/workspaces.mjs', 'lib/workspace-checks.mjs', 'lib/workspace-run.mjs'];

function copyWorkspaceHealth(root) {
  const scripts = path.join(root, 'skills-dev/octocode-dev/scripts');
  fs.mkdirSync(scripts, { recursive: true });
  fs.mkdirSync(path.join(scripts, 'lib'));
  for (const name of ['workspace-health.mjs', 'artifact-freshness.mjs', ...WORKSPACE_HEALTH_LIB]) {
    fs.copyFileSync(fileURLToPath(new URL(`./${name}`, import.meta.url)), path.join(scripts, name));
  }
  const platform = 'packages/octocode-native/bin/platform.cjs';
  fs.mkdirSync(path.dirname(path.join(root, platform)), { recursive: true });
  fs.copyFileSync(fileURLToPath(new URL(`../../../${platform}`, import.meta.url)), path.join(root, platform));
  return scripts;
}

test('repo verify runs each workspace\'s oxlint-free gate and propagates its failure', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-verify-gates-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const scripts = copyWorkspaceHealth(root);
  for (const name of ['dedupe-deps.mjs', 'docs-verify.mjs', 'check-research-guidance.mjs', 'workspace-health.test.mjs']) fs.writeFileSync(path.join(scripts, name), '');
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ workspaces: ['skills/*'] }));
  for (const [name, extra] of [
    ['oxlint-only', {}],
    ['plain', { 'lint:own': 'lint-own' }],
    ['recovery', { verify: 'release-smoke', 'verify:own': 'release-smoke-own' }],
  ]) {
    const dir = path.join(root, 'skills', name);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify({ name, scripts: { build: 'build', lint: 'lint', test: 'test', ...extra } }));
  }
  const bin = path.join(root, 'bin'), trace = path.join(root, 'trace.jsonl');
  fs.mkdirSync(bin);
  fs.writeFileSync(path.join(bin, 'yarn'), `#!${process.execPath}\n` + String.raw`const fs=require('node:fs');const args=process.argv.slice(2);fs.appendFileSync(process.env.TEST_GATE_TRACE,JSON.stringify(args)+'\n');if(args[1]==='recovery'&&args[3]==='verify:own'&&process.env.TEST_GATE_FAIL==='1')process.exit(17);
`, { mode: 0o755 });
  const childEnv = { ...process.env };
  // The subprocess starts its own test runner; inherited node:test state skips it.
  delete childEnv.NODE_TEST_CONTEXT;
  const run = fail => spawnSync(process.execPath, [path.join(scripts, 'workspace-health.mjs'), 'verify'], {
    cwd: root, encoding: 'utf8', env: { ...childEnv, PATH: `${bin}${path.delimiter}${process.env.PATH}`, TEST_GATE_TRACE: trace, TEST_GATE_FAIL: fail ? '1' : '0' }, timeout: 30_000,
  });
  const passing = run(false);
  assert.equal(passing.status, 0, passing.stderr);
  const calls = fs.readFileSync(trace, 'utf8').trim().split('\n').map(JSON.parse);
  // Never `lint` or `verify`: they include oxlint, which the baseline pass runs once. A workspace without `lint:own` has no lint step.
  assert.deepEqual(calls, [
    ['workspace', 'oxlint-only', 'run', 'build'], ['workspace', 'oxlint-only', 'run', 'test'],
    ['workspace', 'plain', 'run', 'build'], ['workspace', 'plain', 'run', 'lint:own'], ['workspace', 'plain', 'run', 'test'],
    ['workspace', 'recovery', 'run', 'verify:own'],
  ]);
  const failing = run(true);
  assert.equal(failing.status, 17, failing.stderr);
});

test('check enforces the config/native layering, keeps mcp-cli standalone and private packages off npm', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-layering-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const scripts = copyWorkspaceHealth(root);
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ workspaces: ['packages/*'] }));
  const scriptSet = Object.fromEntries(['build', 'lint', 'test', 'typecheck', 'verify', 'verify:own'].map(name => [name, name]));
  const write = (dir, pkg) => {
    fs.mkdirSync(path.join(root, 'packages', dir), { recursive: true });
    fs.writeFileSync(path.join(root, 'packages', dir, 'package.json'), JSON.stringify({ scripts: scriptSet, ...pkg }));
  };
  write('native', { name: '@octocodeai/octocode-native' });
  write('config', { name: '@octocodeai/config', private: true });
  write('mcp-cli', { name: 'octocode-mcp-cli', private: true });
  write('mcp', {
    name: 'octocode-mcp',
    dependencies: { '@octocodeai/octocode-native': '1.0.0' },
    devDependencies: { '@octocodeai/config': 'workspace:*' },
  });
  const check = () => spawnSync(process.execPath, [path.join(scripts, 'workspace-health.mjs'), 'check'], { cwd: root, encoding: 'utf8', timeout: 30_000 });
  assert.equal(check().status, 0, check().stderr);

  write('config', { name: '@octocodeai/config', private: true, scripts: { ...scriptSet, 'verify:own': undefined } });
  const withoutOwn = check();
  assert.equal(withoutOwn.status, 1);
  assert.match(withoutOwn.stderr, /packages\/config has "verify" but not "verify:own"/);
  write('config', { name: '@octocodeai/config', private: true });

  write('mcp-cli', { name: 'octocode-mcp-cli', private: true, devDependencies: { '@octocodeai/config': 'workspace:*' } });
  const cliCoupled = check();
  assert.equal(cliCoupled.status, 1);
  assert.match(cliCoupled.stderr, /octocode-mcp-cli must not depend on @octocodeai\/config/);

  write('mcp-cli', { name: 'octocode-mcp-cli', private: true });
  write('mcp', {
    name: 'octocode-mcp',
    dependencies: { '@octocodeai/octocode-native': '1.0.0', '@octocodeai/config': 'workspace:*' },
  });
  const published = check();
  assert.equal(published.status, 1);
  assert.match(published.stderr, /octocode-mcp is published but its dependencies names private @octocodeai\/config/);

  // Any published workspace, not only the layered ones, keeps private packages out of what npm installs.
  write('mcp', { name: 'octocode-mcp', dependencies: { '@octocodeai/octocode-native': '1.0.0' }, devDependencies: { '@octocodeai/config': 'workspace:*' } });
  write('plugin', { name: 'some-plugin', peerDependencies: { 'octocode-mcp-cli': 'workspace:*' } });
  const plugin = check();
  assert.equal(plugin.status, 1);
  assert.match(plugin.stderr, /some-plugin is published but its peerDependencies names private octocode-mcp-cli/);
});

test('run lint:own runs only the workspaces that declare it, so repo-wide lint never reaches oxlint', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-lint-own-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const scripts = copyWorkspaceHealth(root);
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ workspaces: ['skills/*'] }));
  for (const [name, extra] of [['oxlint-only', {}], ['with-own', { 'lint:own': 'lint-own' }]]) {
    const dir = path.join(root, 'skills', name);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify({ name, scripts: { build: 'build', lint: 'lint', test: 'test', ...extra } }));
  }
  const bin = path.join(root, 'bin'), trace = path.join(root, 'trace.jsonl');
  fs.mkdirSync(bin);
  fs.writeFileSync(path.join(bin, 'yarn'), `#!${process.execPath}\n` + String.raw`require('node:fs').appendFileSync(process.env.TEST_GATE_TRACE,JSON.stringify(process.argv.slice(2))+'\n');
`, { mode: 0o755 });
  const childEnv = { ...process.env, PATH: `${bin}${path.delimiter}${process.env.PATH}`, TEST_GATE_TRACE: trace };
  delete childEnv.NODE_TEST_CONTEXT;
  const run = spawnSync(process.execPath, [path.join(scripts, 'workspace-health.mjs'), 'run', 'lint:own'], { cwd: root, encoding: 'utf8', env: childEnv, timeout: 30_000 });
  assert.equal(run.status, 0, run.stderr);
  assert.deepEqual(fs.readFileSync(trace, 'utf8').trim().split('\n').map(JSON.parse), [['workspace', 'with-own', 'run', 'lint:own']]);
});
