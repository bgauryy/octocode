import { test } from 'node:test';
import assert from 'node:assert/strict';
import { diffTrees, diffUnitGraphs, parseTree } from './dev-unify-check.mjs';

const profile = name => ({ name, opt_level: '0', debuginfo: 0, debug_assertions: true, panic: 'unwind' });
const unit = (pkg, features, deps = [], { mode = 'build', profileName = 'dev' } = {}) => ({
  pkg_id: `registry+https://github.com/rust-lang/crates.io-index#${pkg}@1.0.0`,
  target: { name: pkg.replace(/-/g, '_'), kind: ['lib'] }, profile: profile(profileName), platform: null, mode, features,
  dependencies: deps.map(index => ({ index })),
});
// tokio-util ← octocode-native ← octocode-cli
const buildGraph = { units: [unit('tokio-util', ['codec']), unit('octocode-native', [], [0]), unit('octocode-cli', [], [1])] };

test('a test graph that reuses every shared unit (profile name aside) passes; test-only crates never count', () => {
  const test = { units: [
    unit('tokio-util', ['codec'], [], { profileName: 'test' }), unit('octocode-native', [], [0], { profileName: 'test' }),
    unit('wiremock', []), unit('octocode-native', [], [1, 2], { mode: 'test', profileName: 'test' }),
  ] };
  assert.deepEqual(diffUnitGraphs(buildGraph, test), { root: [], derived: [], exempted: [] });
});

test('a shared crate with a feature only the test graph enables drifts, and its dependents rebuild', () => {
  const build = { units: [unit('tokio-util', []), unit('octocode-native', [], [0]), unit('octocode-cli', [], [1])] };
  const test = { units: [unit('tokio-util', ['codec']), unit('octocode-native', [], [0]), unit('octocode-cli', [], [1])] };
  const result = diffUnitGraphs(build, test);
  assert.deepEqual(result.root.map(row => [row.package, row.features]), [['tokio-util', ['codec']]]);
  assert.deepEqual(result.derived.map(row => row.package), ['octocode-native', 'octocode-cli']);
});

test('an exempt package and the dependents only it drifts are reported, not failed', () => {
  const build = { units: [unit('napi', ['napi4']), unit('octocode-runtime-napi', [], [0])] };
  const test = { units: [unit('napi', ['napi4', 'noop']), unit('octocode-runtime-napi', [], [0])] };
  const result = diffUnitGraphs(build, test, new Map([['napi', 'test-only noop']]));
  assert.deepEqual([result.root, result.derived], [[], []]);
  assert.deepEqual(result.exempted.map(row => row.package), ['napi', 'octocode-runtime-napi']);
});

test('cargo tree fallback: a shared package whose test feature set the build never compiles drifts', () => {
  const build = parseTree('octocode-cli v20.0.0 (/x/cli)|\ntokio-util v0.7.1|default\nnapi v3.1.0|napi4\n');
  const match = parseTree('tokio-util v0.7.1|default (*)\nwiremock v0.6.0|\nnapi v3.1.0|napi4,noop\n');
  const exempt = new Map([['napi', 'test-only noop']]);
  assert.deepEqual(diffTrees(build, match, exempt).root, []);
  assert.deepEqual(diffTrees(build, match, exempt).exempted.map(row => row.package), ['napi']);
  const missing = parseTree('tokio-util v0.7.1|codec,default\n');
  assert.deepEqual(diffTrees(build, missing, exempt).root.map(row => [row.package, row.features]), [['tokio-util', ['codec', 'default']]]);
});
