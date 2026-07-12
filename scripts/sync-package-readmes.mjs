#!/usr/bin/env node
import { copyFileSync, existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const rootDir = resolve(__dirname, '..');
const rootReadme = join(rootDir, 'README.md');
const packagesDir = join(rootDir, 'packages');

// Packages with hand-authored READMEs that must never be overwritten by this
// script. They are committed to git and explicitly un-ignored in .gitignore
// via the !/packages/<name>/README.md rules.
const PROTECTED_PACKAGES = new Set([
  'octocode-awareness',
]);

// Every other public (non-`private`) package under packages/ gets the shared
// root README synced into its own README.md at build/prepack time — this is
// what actually ships on its npm page. Discovered dynamically so a new
// package is covered automatically instead of silently shipping stale or
// missing docs.
function discoverSyncTargets() {
  return readdirSync(packagesDir, { withFileTypes: true })
    .filter(entry => entry.isDirectory() && !PROTECTED_PACKAGES.has(entry.name))
    .filter(entry => existsSync(join(packagesDir, entry.name, 'package.json')))
    .filter(entry => {
      const pkg = JSON.parse(
        readFileSync(join(packagesDir, entry.name, 'package.json'), 'utf-8')
      );
      return pkg.private !== true;
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
  const packageName = relative('packages', target);
  if (PROTECTED_PACKAGES.has(packageName)) {
    throw new Error(
      `Cannot sync root README.md to ${target} — it has a hand-authored README.md ` +
      `committed to git. Remove it from the sync target list.`,
    );
  }

  const packageDir = join(rootDir, target);
  const packageJsonPath = join(packageDir, 'package.json');

  if (!existsSync(packageJsonPath) || !statSync(packageDir).isDirectory()) {
    throw new Error(`Expected a package directory with package.json: ${packageDir}`);
  }

  copyFileSync(rootReadme, join(packageDir, 'README.md'));
  console.log(`✓ README.md synced to ${relative(rootDir, packageDir)}`);
}
