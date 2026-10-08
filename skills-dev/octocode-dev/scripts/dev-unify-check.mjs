#!/usr/bin/env node
// Opt-in drift check for the native `dev-unify` feature (crates/runtime/Cargo.toml).
// `build:dev` and `test:rust` should compile every shared crate once: a crate
// both selections build, but with a different unit (features or profile
// settings) in the test selection, is recompiled by every `cargo test` after
// `build:dev`. Fails when such a crate is not an approved exception.
//
// Source of truth: nightly `cargo --unit-graph` (units with features and
// profile settings). Without nightly it falls back to `cargo tree` feature
// sets, which cannot see profile drift. Without cargo it skips (exit 0).
//
// Usage: node skills-dev/octocode-dev/scripts/dev.mjs check:dev-unify [--json] [--tree]
// (`--tree` forces the `cargo tree` fallback.)
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const NATIVE = path.resolve(HERE, '..', '..', '..', 'packages', 'octocode-native');

/** `build:dev` hosts selection (packages/octocode-native/scripts/build-native.cjs `hostsCommand`). */
export const BUILD_SELECTION = ['--locked', '-p', 'octocode-cli', '-p', 'octocode-runtime-napi', '--features', 'octocode-native/dev-unify'];
/** First `test:rust` command (packages/octocode-native/package.json); the engine `--all-features` run is a separate selection by design. */
export const TEST_SELECTION = [
  '--locked', '-p', 'octocode-native', '-p', 'octocode-github', '-p', 'octocode-cli', '-p', 'octocode-runtime-napi',
  '--features', 'octocode-runtime-napi/napi-test,octocode-native/dev-unify',
];
/** Packages allowed to differ, with the reason. */
export const EXEMPT = new Map([
  ['napi', "napi-test turns on napi/noop for tests only; dev-unify deliberately excludes it (crates/runtime/Cargo.toml)"],
  ['octocode-runtime-napi', 'its own napi-test feature (same reason as napi)'],
]);

const TEST_MODES = new Set(['test', 'doctest']);
const packageName = pkgId => {
  // "registry+https://…#name@1.2.3", "path+file:///…/crates/cli#octocode-cli@20.0.0", or "path+file:///…/name#1.2.3".
  const tail = pkgId.slice(pkgId.lastIndexOf('#') + 1);
  const at = tail.lastIndexOf('@');
  if (at > 0) return tail.slice(0, at);
  return pkgId.slice(0, pkgId.lastIndexOf('#')).split('/').pop();
};

/** Own identity of a unit: everything but the profile's name (`dev` vs `test` with equal settings compile alike). */
function unitKey(unit) {
  const { name: _name, ...profile } = unit.profile ?? {};
  const settings = Object.fromEntries(Object.entries(profile).sort(([a], [b]) => a.localeCompare(b)));
  return JSON.stringify([unit.pkg_id, unit.target?.name, unit.target?.kind, unit.mode, [...(unit.features ?? [])].sort(), settings, unit.platform ?? null]);
}

/**
 * Units of the test graph that rebuild a crate the build graph also compiles.
 * `root` units differ themselves; `derived` units match but depend on a
 * drifted unit. Test-only crates (absent from the build graph) never count.
 */
export function diffUnitGraphs(build, test, exempt = EXEMPT) {
  const buildKeys = new Set(build.units.map(unitKey));
  const buildPackages = new Set(build.units.map(unit => packageName(unit.pkg_id)));
  // Recursive identity: a digest of a unit's own key and its dependencies' identities.
  const identities = graph => {
    const memo = new Map();
    const identity = index => {
      if (!memo.has(index)) {
        const unit = graph.units[index];
        const deps = (unit.dependencies ?? []).map(dep => identity(dep.index)).sort();
        memo.set(index, createHash('sha256').update(JSON.stringify([unitKey(unit), deps])).digest('hex'));
      }
      return memo.get(index);
    };
    return graph.units.map((_, index) => identity(index));
  };
  const buildIds = new Set(identities(build));
  const testIds = identities(test);
  const shared = index => {
    const unit = test.units[index];
    return !TEST_MODES.has(unit.mode) && buildPackages.has(packageName(unit.pkg_id));
  };
  const ownDrift = index => shared(index) && !buildKeys.has(unitKey(test.units[index]));
  // Whether a non-exempt crate's own drift reaches this unit.
  const caused = new Map();
  const unexplained = index => {
    if (!caused.has(index)) {
      const unit = test.units[index];
      const own = ownDrift(index) && !exempt.has(packageName(unit.pkg_id));
      caused.set(index, own || (unit.dependencies ?? []).some(dep => unexplained(dep.index)));
    }
    return caused.get(index);
  };
  const root = [], derived = [], exempted = [];
  test.units.forEach((unit, index) => {
    if (!shared(index) || buildIds.has(testIds[index])) return;
    const name = packageName(unit.pkg_id);
    const row = { package: name, target: unit.target?.name, mode: unit.mode, features: [...(unit.features ?? [])].sort() };
    if (!unexplained(index)) exempted.push(row);
    else if (ownDrift(index)) root.push(row);
    else derived.push(row);
  });
  return { root, derived, exempted };
}

/** `cargo tree --prefix none -f '{p}|{f}'` → package → set of feature lists seen. */
export function parseTree(text) {
  const packages = new Map();
  for (const raw of text.split('\n')) {
    const line = raw.replace(/ \(\*\)$/, '').trim();
    const bar = line.lastIndexOf('|');
    if (bar < 0) continue;
    const name = line.slice(0, bar).split(' ')[0];
    const features = line.slice(bar + 1).split(',').filter(Boolean).sort().join(',');
    if (!packages.has(name)) packages.set(name, new Set());
    packages.get(name).add(features);
  }
  return packages;
}

/** Shared packages whose test feature set is not one the build graph compiles. */
export function diffTrees(build, test, exempt = EXEMPT) {
  const root = [], exempted = [];
  for (const [name, variants] of test) {
    const built = build.get(name);
    if (!built) continue;
    for (const features of variants) {
      if (built.has(features)) continue;
      const row = { package: name, features: features ? features.split(',') : [], built: [...built].map(f => (f ? f.split(',') : [])) };
      (exempt.has(name) ? exempted : root).push(row);
    }
  }
  return { root, derived: [], exempted };
}

function cargo(args, toolchain) {
  const full = toolchain ? [`+${toolchain}`, ...args] : args;
  return spawnSync('cargo', full, { cwd: NATIVE, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
}

function hasNightly() {
  const probe = cargo(['--version'], 'nightly');
  return !probe.error && probe.status === 0;
}

function collect(forceTree) {
  if (!forceTree && hasNightly()) {
    const graph = (command, extra, selection) => {
      const run = cargo([command, ...extra, '--unit-graph', '-Z', 'unstable-options', ...selection], 'nightly');
      if (run.status !== 0) throw Error(`cargo +nightly ${command} --unit-graph failed:\n${run.stderr}`);
      return JSON.parse(run.stdout);
    };
    const build = graph('build', ['--bins', '--lib'], BUILD_SELECTION);
    const test = graph('test', ['--no-run'], TEST_SELECTION);
    return { method: 'unit-graph (nightly)', ...diffUnitGraphs(build, test) };
  }
  const tree = (edges, selection) => {
    const run = cargo(['tree', '-e', edges, '--prefix', 'none', '-f', '{p}|{f}', ...selection]);
    if (run.status !== 0) throw Error(`cargo tree failed:\n${run.stderr}`);
    return parseTree(run.stdout);
  };
  return { method: 'cargo tree features (profile drift unchecked)', ...diffTrees(tree('normal,build', BUILD_SELECTION), tree('normal,build,dev', TEST_SELECTION)) };
}

function main() {
  const probe = cargo(['--version']);
  if (probe.error) {
    console.log('dev-unify check skipped: cargo is not installed.');
    return 0;
  }
  const result = collect(process.argv.includes('--tree'));
  if (process.argv.includes('--json')) console.log(JSON.stringify(result, null, 2));
  else {
    console.log(`dev-unify check via ${result.method}`);
    for (const row of result.exempted) console.log(`  exempt  ${row.package} ${row.target ?? ''} [${row.features.join(',')}]${EXEMPT.has(row.package) ? `: ${EXEMPT.get(row.package)}` : ' (an exempt dependency drifted)'}`);
    for (const row of result.root) console.log(`  DRIFT   ${row.package} ${row.target ?? ''} ${row.mode ?? ''} [${row.features.join(',')}]`);
    if (result.derived.length) console.log(`  ${result.derived.length} more shared unit(s) rebuild because a dependency drifted.`);
  }
  if (result.root.length || result.derived.length) {
    console.error('dev-unify drift: `cargo test` recompiles crates `build:dev` already built. Add the missing features to `dev-unify` in packages/octocode-native/crates/runtime/Cargo.toml, or exempt the package here with a reason.');
    return 1;
  }
  console.log('dev-unify: the test selection reuses every shared crate the dev build compiles.');
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main();
