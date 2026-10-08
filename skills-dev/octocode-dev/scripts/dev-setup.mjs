#!/usr/bin/env node
/**
 * dev-setup.mjs — pin workspace packages and the sibling octocode-core locally.
 *
 * Adds the monorepo-internal packages and consolidated native platform packages
 * to the root package.json `resolutions` field so Yarn resolves them from the
 * local workspace (not from the npm registry) during development. Any transitive
 * consumer of these packages will also get the local build, giving you a single
 * consistent source of truth in dev mode.
 *
 * Usage:
 *   node skills-dev/octocode-dev/scripts/dev.mjs setup            (task runner)
 *   node ./skills-dev/octocode-dev/scripts/dev-setup.mjs
 *
 * Undo / publish prep:
 *   node ./skills-dev/octocode-dev/scripts/prepublish.mjs --fix
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import {
  OCTOCODE_CORE_PACKAGE,
  AGENT_TESTING_PACKAGE,
  isLocalResolution,
  localAgentTestingResolution,
  localCoreResolution,
  managedResolutionPackages,
  workspaceResolutionPackages,
} from './dev-resolution-contract.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const PKG_PATH = join(ROOT, 'package.json');
const NATIVE_PKG_PATH = join(ROOT, 'packages/octocode-native/package.json');
const argv = process.argv.slice(2);
const flags = {
  dryRun: argv.includes('--dry-run') || argv.includes('-n'),
  install: argv.includes('--install') || argv.includes('-i'),
  reset: argv.includes('--reset') || argv.includes('--unlink'),
};
const knownArgs = new Set([
  '--dry-run',
  '-n',
  '--install',
  '-i',
  '--reset',
  '--unlink',
]);
for (const arg of argv) {
  if (!knownArgs.has(arg)) {
    console.error(`✖ unknown argument: ${arg}`);
    process.exit(1);
  }
}

const nativePkg = JSON.parse(readFileSync(NATIVE_PKG_PATH, 'utf8'));

/** Packages that should resolve to this workspace during development. */
const WORKSPACE_RESOLUTIONS = Object.fromEntries(
  workspaceResolutionPackages(nativePkg).map(name => [name, 'workspace:*'])
);
const coreResolution = localCoreResolution(ROOT);
const agentTestingResolution = localAgentTestingResolution(ROOT);
const DEV_RESOLUTIONS = {
  ...WORKSPACE_RESOLUTIONS,
  ...(coreResolution ? { [OCTOCODE_CORE_PACKAGE]: coreResolution } : {}),
  ...(agentTestingResolution
    ? { [AGENT_TESTING_PACKAGE]: agentTestingResolution }
    : {}),
};

const pkg = JSON.parse(readFileSync(PKG_PATH, 'utf8'));
pkg.resolutions ??= {};

if (flags.reset) {
  const removed = [];
  for (const name of managedResolutionPackages(nativePkg)) {
    if (!isLocalResolution(pkg.resolutions[name])) continue;
    delete pkg.resolutions[name];
    removed.push(name);
  }
  if (Object.keys(pkg.resolutions).length === 0) delete pkg.resolutions;
  if (!flags.dryRun) {
    writeFileSync(PKG_PATH, JSON.stringify(pkg, null, 2) + '\n');
  }
  console.log(
    removed.length > 0
      ? `${flags.dryRun ? 'Would remove' : 'Removed'} ${removed.length} local dev resolution(s):\n  ${removed.join('\n  ')}`
      : 'No local Octocode dev resolutions to remove.'
  );
  process.exit(0);
}

const added = [];
const alreadySet = [];

for (const [name, spec] of Object.entries(DEV_RESOLUTIONS)) {
  if (pkg.resolutions[name] === spec) {
    alreadySet.push([name, spec]);
  } else {
    pkg.resolutions[name] = spec;
    added.push(name);
  }
}

const removedStale = [];
// A missing sibling keeps a registry resolution, but a local link to it is
// stale: it would make `yarn install` fail on a path that no longer exists.
for (const [name, resolution, sibling] of [
  [
    OCTOCODE_CORE_PACKAGE,
    coreResolution,
    '../octocode-mcp-host/packages/octocode-core',
  ],
  [
    AGENT_TESTING_PACKAGE,
    agentTestingResolution,
    '../octocode-agent/packages/octocode-agent-testing',
  ],
]) {
  if (resolution) continue;
  if (isLocalResolution(pkg.resolutions[name])) {
    delete pkg.resolutions[name];
    removedStale.push(name);
    console.warn(
      `⚠ ${name} sibling not found at ${sibling}; removed its stale local resolution.`
    );
  } else {
    console.warn(
      `⚠ ${name} sibling not found at ${sibling}; keeping the current registry resolution.`
    );
  }
}

if (added.length === 0 && removedStale.length === 0) {
  console.log('✓ Local dev resolutions already set — nothing to do.');
  for (const [name, spec] of alreadySet) {
    console.log(`  · resolutions.${name}: "${spec}"`);
  }
  process.exit(0);
}

if (!flags.dryRun) {
  pkg.resolutions = Object.fromEntries(
    Object.entries(pkg.resolutions).sort(([a], [b]) => a.localeCompare(b))
  );
  writeFileSync(PKG_PATH, JSON.stringify(pkg, null, 2) + '\n');
}

console.log(
  `${flags.dryRun ? 'Would add' : '✓ Added'} local resolutions to root package.json:`
);
for (const name of added) {
  console.log(`  + resolutions.${name}: "${DEV_RESOLUTIONS[name]}"`);
}
for (const name of removedStale) {
  console.log(`  - resolutions.${name} (stale local link)`);
}
if (alreadySet.length > 0) {
  console.log('\n  Already set:');
  for (const [name, spec] of alreadySet) {
    console.log(`  · resolutions.${name}: "${spec}"`);
  }
}
if (flags.install && !flags.dryRun) {
  process.exit(
    spawnSync('yarn', ['install'], { cwd: ROOT, stdio: 'inherit' }).status ?? 0
  );
}
console.log('\n  Run `yarn install` to apply the new resolutions.');
console.log(
  '  Run `node ./skills-dev/octocode-dev/scripts/prepublish.mjs --fix` before publishing to undo.\n'
);
