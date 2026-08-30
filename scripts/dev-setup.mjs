#!/usr/bin/env node
/**
 * dev-setup.mjs — pin workspace packages and the sibling octocode-core locally.
 *
 * Adds the monorepo-internal packages and the octocode-engine platform packages
 * to the root package.json `resolutions` field so Yarn resolves them from the
 * local workspace (not from the npm registry) during development. Any transitive
 * consumer of these packages will also get the local build, giving you a single
 * consistent source of truth in dev mode.
 *
 * Usage:
 *   yarn devScript            (via root scripts)
 *   node ./scripts/dev-setup.mjs
 *
 * Undo / publish prep:
 *   node ./scripts/prepublish.mjs --fix
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import {
  OCTOCODE_CORE_PACKAGE,
  isLocalResolution,
  localCoreResolution,
  managedResolutionPackages,
  workspaceResolutionPackages,
} from './dev-resolution-contract.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const PKG_PATH = join(ROOT, 'package.json');
const ENGINE_PKG_PATH = join(ROOT, 'packages/octocode-engine/package.json');
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

const enginePkg = JSON.parse(readFileSync(ENGINE_PKG_PATH, 'utf8'));

/** Packages that should resolve to this workspace during development. */
const WORKSPACE_RESOLUTIONS = Object.fromEntries(
  workspaceResolutionPackages(enginePkg).map(name => [name, 'workspace:*'])
);
const coreResolution = localCoreResolution(ROOT);
const DEV_RESOLUTIONS = {
  ...WORKSPACE_RESOLUTIONS,
  ...(coreResolution ? { [OCTOCODE_CORE_PACKAGE]: coreResolution } : {}),
};

const pkg = JSON.parse(readFileSync(PKG_PATH, 'utf8'));
pkg.resolutions ??= {};

if (flags.reset) {
  const removed = [];
  for (const name of managedResolutionPackages(enginePkg)) {
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

if (!coreResolution) {
  console.warn(
    `⚠ ${OCTOCODE_CORE_PACKAGE} sibling not found at ../octocode-mcp-host/packages/octocode-core; keeping the current registry resolution.`
  );
}

if (added.length === 0) {
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
  console.log(`  + resolutions.${name}: "workspace:*"`);
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
  '  Run `node ./scripts/prepublish.mjs --fix` before publishing to undo.\n'
);
