#!/usr/bin/env node
/**
 * dev-setup.mjs — pin local workspace packages to workspace:* in root resolutions.
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

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const PKG_PATH = join(ROOT, 'package.json');
const ENGINE_PKG_PATH = join(ROOT, 'packages/octocode-engine/package.json');

const enginePkg = JSON.parse(readFileSync(ENGINE_PKG_PATH, 'utf8'));
const enginePlatformPackages = Object.keys(enginePkg.optionalDependencies ?? {}).filter((name) =>
  name.startsWith('@octocodeai/octocode-engine-')
);

/** Packages that should resolve to the local workspace during development. */
const WORKSPACE_RESOLUTIONS = Object.fromEntries(
  [
    '@octocodeai/octocode-tools-core',
    '@octocodeai/config',
    '@octocodeai/octocode-engine',
    ...enginePlatformPackages,
  ].map((name) => [name, 'workspace:*'])
);

const pkg = JSON.parse(readFileSync(PKG_PATH, 'utf8'));
pkg.resolutions ??= {};

const added = [];
const alreadySet = [];

for (const [name, spec] of Object.entries(WORKSPACE_RESOLUTIONS)) {
  if (pkg.resolutions[name] === spec) {
    alreadySet.push(name);
  } else {
    pkg.resolutions[name] = spec;
    added.push(name);
  }
}

if (added.length === 0) {
  console.log('✓ workspace:* resolutions already set — nothing to do.');
  for (const name of alreadySet) {
    console.log(`  · resolutions.${name}: "workspace:*"`);
  }
  process.exit(0);
}

writeFileSync(PKG_PATH, JSON.stringify(pkg, null, 2) + '\n');

console.log('✓ Added workspace:* resolutions to root package.json:');
for (const name of added) {
  console.log(`  + resolutions.${name}: "workspace:*"`);
}
if (alreadySet.length > 0) {
  console.log('\n  Already set:');
  for (const name of alreadySet) {
    console.log(`  · resolutions.${name}: "workspace:*"`);
  }
}
console.log('\n  Run `yarn install` to apply the new resolutions.');
console.log('  Run `node ./scripts/prepublish.mjs --fix` before publishing to undo.\n');
