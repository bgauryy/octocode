#!/usr/bin/env node
import { copyFileSync, existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const rootDir = resolve(__dirname, '..');
const rootReadme = join(rootDir, 'README.md');
const packagesDir = join(rootDir, 'packages');

function readPackageManifest(packageDir) {
  return JSON.parse(readFileSync(join(packageDir, 'package.json'), 'utf-8'));
}

function ownsReadme(packageManifest) {
  return packageManifest.octocode?.readmeSync === false;
}

// Every public (non-`private`) package that does not own its README gets the shared
// root README synced into its own README.md at build/prepack time — this is
// what actually ships on its npm page. Discovered dynamically so a new
// package is covered automatically instead of silently shipping stale or
// missing docs. Packages opt out through package.json#octocode.readmeSync.
function discoverSyncTargets() {
  return readdirSync(packagesDir, { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .filter(entry => existsSync(join(packagesDir, entry.name, 'package.json')))
    .filter(entry => {
      const pkg = readPackageManifest(join(packagesDir, entry.name));
      return pkg.private !== true && !ownsReadme(pkg);
    })
    .map(entry => join('packages', entry.name))
    .sort();
}

const requestedTarget = process.argv[2];
const targets = requestedTarget
  ? [
      requestedTarget === '.'
        ? relative(rootDir, process.cwd())
        : requestedTarget,
    ]
  : discoverSyncTargets();

for (const target of targets) {
  const packageDir = join(rootDir, target);
  const packageJsonPath = join(packageDir, 'package.json');

  if (!existsSync(packageJsonPath) || !statSync(packageDir).isDirectory()) {
    throw new Error(`Expected a package directory with package.json: ${packageDir}`);
  }

  if (ownsReadme(readPackageManifest(packageDir))) {
    console.log(`✓ Package owns README.md in ${relative(rootDir, packageDir)}`);
    continue;
  }

  copyFileSync(rootReadme, join(packageDir, 'README.md'));
  console.log(`✓ README.md synced to ${relative(rootDir, packageDir)}`);
}
