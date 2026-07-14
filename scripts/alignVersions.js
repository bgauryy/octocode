#!/usr/bin/env node

import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const DRY_RUN = process.argv.includes('--dry-run');
const CHECK = process.argv.includes('--check');

const TARGET_PACKAGE_DIRS = [
  'packages/octocode-tools-core',
  'packages/octocode-mcp',
  'packages/octocode-engine',
  'packages/octocode-config',
  'packages/octocode',
];

const ENGINE_NPM_DIR = 'packages/octocode-engine/npm';

function readJson(file) {
  return JSON.parse(readFileSync(file, 'utf8'));
}

function writeJson(file, json) {
  writeFileSync(file, `${JSON.stringify(json, null, 2)}\n`);
}

function collectPackageJsonPaths() {
  const paths = TARGET_PACKAGE_DIRS.map((dir) => join(ROOT, dir, 'package.json'));
  const engineNpmRoot = join(ROOT, ENGINE_NPM_DIR);

  if (existsSync(engineNpmRoot)) {
    for (const entry of readdirSync(engineNpmRoot, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      paths.push(join(engineNpmRoot, entry.name, 'package.json'));
    }
  }

  return [...new Set(paths)].sort();
}

const rootPkgPath = join(ROOT, 'package.json');
const rootPkg = readJson(rootPkgPath);
const rootVersion = rootPkg.version;

if (!rootVersion) {
  console.error('Root package.json has no version field.');
  process.exit(1);
}

const packageJsonPaths = collectPackageJsonPaths();
const missingPaths = packageJsonPaths.filter((file) => !existsSync(file));

if (missingPaths.length > 0) {
  console.error('Missing target package.json file(s):');
  for (const file of missingPaths) console.error(`  - ${relative(ROOT, file)}`);
  process.exit(1);
}

let changed = 0;

console.log(`Aligning package versions to ${rootVersion}${DRY_RUN ? ' (dry-run)' : CHECK ? ' (check)' : ''}\n`);

for (const file of packageJsonPaths) {
  const pkg = readJson(file);
  const currentVersion = pkg.version;
  const relPath = relative(ROOT, file);

  if (!pkg.name) {
    console.error(`${relPath}: missing name field`);
    process.exitCode = 1;
    continue;
  }

  if (currentVersion === rootVersion) {
    console.log(`✓ ${pkg.name} already ${rootVersion}`);
    continue;
  }

  changed += 1;
  console.log(`${CHECK || DRY_RUN ? '~' : '✓'} ${pkg.name}: ${currentVersion ?? '<missing>'} → ${rootVersion}`);

  if (!CHECK && !DRY_RUN) {
    pkg.version = rootVersion;
    writeJson(file, pkg);
  }
}

if (CHECK && changed > 0) {
  console.error(`\n${changed} package version(s) are not aligned. Run node ./scripts/alignVersions.js to fix.`);
  process.exit(1);
}

if (DRY_RUN) {
  console.log(`\n${changed} package version(s) would be updated.`);
} else if (!CHECK) {
  console.log(`\nUpdated ${changed} package version(s).`);
}
